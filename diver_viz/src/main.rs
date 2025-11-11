use bevy::{
    camera::Exposure,
    core_pipeline::tonemapping::Tonemapping,
    input::gamepad,
    light::{AtmosphereEnvironmentMapLight, CascadeShadowConfigBuilder, light_consts::lux},
    pbr::{Atmosphere, AtmosphereSettings},
    post_process::bloom::Bloom,
    prelude::*,
};
use bevy_http_client::prelude::*;
use csgrs::traits::CSG;
use geozero::GeomProcessor;
use geozero::mvt::tile::Layer;
use geozero::mvt::{Message, Tile};

const MARTIN_MVT_ENDPOINT: &str = "http://localhost:3000/denver_blocks_all_zoom_15_up";

#[derive(Debug, Clone)]
struct Building {
    geometry: Vec<Vec<Vec2>>,
    height: Option<f64>,
}

#[derive(Debug, Clone)]
struct Road {
    points: Vec<Vec2>,
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Window {
                    title: "denver-diver".to_string(),
                    fit_canvas_to_parent: true,
                    ..default()
                }
                .into(),
                ..default()
            }),
        )
        .add_plugins(HttpClientPlugin)
        .insert_resource(ClearColor(Color::srgb(0.82, 0.73, 0.86)))
        .add_systems(Startup, (spawn_player_camera, request_tiles))
        .add_systems(
            Update,
            (
                camera_update,
                on_tile_response,
                on_tile_error,
                dynamic_scene,
            ),
        )
        .run();
}

fn dynamic_scene(
    mut suns: Query<&mut Transform, With<DirectionalLight>>,
    gamepads: Query<&Gamepad>,
    time: Res<Time>,
) {
    for mut tf in suns {
        for gamepad in gamepads {
            if gamepad.pressed(GamepadButton::RightTrigger) {
                tf.rotate_x(-time.delta_secs() * std::f32::consts::PI / 10.0);
            }
            if gamepad.pressed(GamepadButton::LeftTrigger) {
                tf.rotate_x(time.delta_secs() * std::f32::consts::PI / 10.0);
            }
        }
    }
}

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d { ..default() },
        Transform::from_xyz(0.0, 1.0, 0.0).looking_at(
            Vec3 {
                x: 1.0,
                y: 1.0,
                z: 0.0,
            },
            Vec3::Y,
        ),
        Atmosphere::EARTH,
        AtmosphereSettings {
            aerial_view_lut_max_distance: 3.2e5,
            scene_units_to_m: 1e+4,
            ..Default::default()
        },
        Exposure::SUNLIGHT,
        Tonemapping::AcesFitted,
        Bloom::NATURAL,
        AtmosphereEnvironmentMapLight::default(),
    ));
}

const CAM_SENSITIVITY_X: f32 = 1.1;
const CAM_SENSITIVITY_Y: f32 = 0.7;
const SPEED: f32 = 12.0;

fn camera_update(
    camera_transform: Query<&mut Transform, With<Camera3d>>,
    gamepads: Query<&Gamepad>,
    timer: Res<Time>,
) {
    for mut cam in camera_transform {
        for gamepad in gamepads {
            // movement
            let move_vec = if gamepad.left_stick().length() > gamepad.dpad().length() {
                gamepad.left_stick()
            } else {
                gamepad.dpad()
            } * SPEED
                * timer.delta_secs();
            let offset = move_vec.x * cam.local_x() + move_vec.y * -1.0 * cam.local_z();
            if gamepad.left_stick().length() > 0.01 {
                cam.translation += offset;
            }

            let mut right_stick = gamepad.right_stick();
            right_stick.x *= CAM_SENSITIVITY_X;
            right_stick.y *= CAM_SENSITIVITY_Y;
            if right_stick.length() > 0.1 {
                cam.rotate_y(-1.0 * right_stick.x * timer.delta_secs());
                cam.rotate_local_x(right_stick.y * timer.delta_secs());
            }
        }
    }
}

const TILE_COORD_Z: u32 = 15;
const TILE_COORD_X: u32 = 6827;
const TILE_COORD_Y: u32 = 12436;

fn request_tiles(mut ev_request: MessageWriter<HttpRequest>) {
    let url = format!(
        "{}/{}/{}/{}",
        MARTIN_MVT_ENDPOINT, TILE_COORD_Z, TILE_COORD_X, TILE_COORD_Y
    );
    match HttpClient::new().get(url).try_build() {
        Ok(request) => {
            ev_request.write(request);
        }
        Err(e) => {
            eprintln!("Failed to build request: {}", e);
        }
    }
}

fn on_tile_response(
    mut ev_resp: MessageReader<HttpResponse>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for response in ev_resp.read() {
        let bytes = response.bytes.as_slice();
        let tile = Tile::decode(bytes).unwrap();

        let mut buildings = Vec::new();
        let mut landuse = Vec::new();
        let mut roads = Vec::new();

        for layer in &tile.layers {
            // could also grab layer extent here, don't think we care tho
            if layer.name == "buildings" {
                info!("Processing buildings layer...");
                for feature in &layer.features {
                    let mut processor = BuildingProcessor::new(TILE_COORD_X, TILE_COORD_Y);
                    let height: Option<f64> =
                        extract_tag_value_as_f64(&feature.tags, layer, "height".to_string());
                    if geozero::mvt::process_geom(feature, &mut processor).is_ok() {
                        if let Some(mut building) = processor.building {
                            // add height we snagged from the tags
                            building.height = height;
                            buildings.push(building);
                        }
                    }
                }
            } else if layer.name == "roads" {
                info!("Processing roads layer...");
                for feature in &layer.features {
                    let mut processor = RoadProcessor::new(TILE_COORD_X, TILE_COORD_Y);
                    if geozero::mvt::process_geom(feature, &mut processor).is_ok() {
                        roads.extend(processor.roads);
                    }
                }
            } else if layer.name == "landuse" {
                // NOTE: This just uses the same processing as buildings for now.
                // Should read the tags and do some different stuff here for diff types
                info!("Processing landuse layer...");
                for feature in &layer.features {
                    let mut processor = BuildingProcessor::new(TILE_COORD_X, TILE_COORD_Y);
                    if geozero::mvt::process_geom(feature, &mut processor).is_ok() {
                        if let Some(mut building) = processor.building {
                            // add hardcoded height for now
                            building.height = Some(0.1);
                            landuse.push(building);
                        }
                    }
                }
            }
        }

        info!("✓ Parsed {} building polygons", buildings.len());
        info!("✓ Parsed {} landuse polygons", landuse.len());
        info!("✓ Parsed {} roads", roads.len());

        // Convert all building points to world coordinates
        for building in &mut buildings {
            for ring in &mut building.geometry {
                for point in ring.iter_mut() {
                    *point = BuildingProcessor::tile_to_world_static(
                        point.x as f64,
                        point.y as f64,
                        TILE_COORD_X,
                        TILE_COORD_Y,
                        1000.0,
                    );
                }
            }
        }

        // Convert all landuse points to world coordinates
        for landuse_poly in &mut landuse {
            for ring in &mut landuse_poly.geometry {
                for point in ring.iter_mut() {
                    *point = BuildingProcessor::tile_to_world_static(
                        point.x as f64,
                        point.y as f64,
                        TILE_COORD_X,
                        TILE_COORD_Y,
                        1000.0,
                    );
                }
            }
        }

        // Convert road points to world coordinates
        for road in &mut roads {
            for point in road.points.iter_mut() {
                *point = RoadProcessor::tile_to_world_static(
                    point.x as f64,
                    point.y as f64,
                    TILE_COORD_X,
                    TILE_COORD_Y,
                );
            }
        }

        // Compute center of all objects
        let mut min = Vec2::splat(f32::MAX);
        let mut max = Vec2::splat(f32::MIN);

        for building in &buildings {
            for ring in &building.geometry {
                for point in ring {
                    min = min.min(*point);
                    max = max.max(*point);
                }
            }
        }
        for road in &roads {
            for point in &road.points {
                min = min.min(*point);
                max = max.max(*point);
            }
        }

        let center = (min + max) / 2.0;
        info!("Computed world center: {:?}", center);

        // Apply center offset to buildings and roads
        for building in &mut buildings {
            for ring in &mut building.geometry {
                for point in ring.iter_mut() {
                    *point -= center;
                }
            }
        }
        for landuse_poly in &mut landuse {
            for ring in &mut landuse_poly.geometry {
                for point in ring.iter_mut() {
                    *point -= center;
                }
            }
        }
        for road in &mut roads {
            for point in &mut road.points {
                *point -= center;
            }
        }

        let building_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.64, 0.55),
            metallic: 0.0,
            perceptual_roughness: 0.9,
            ..default()
        });

        let landuse_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.4, 0.54, 0.75),
            metallic: 0.0,
            perceptual_roughness: 1.0,
            ..default()
        });

        let road_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.98, 0.37, 0.43),
            metallic: 1.0,
            perceptual_roughness: 0.0,
            ..default()
        });

        // Spawn building meshes
        for building in &buildings {
            if let Some(mesh) = create_building_mesh(building) {
                commands.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(building_material.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ));
            }
        }

        // Spawn landuse meshes
        for landuse_poly in &landuse {
            if let Some(mesh) = create_building_mesh(landuse_poly) {
                commands.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(landuse_material.clone()),
                    Transform::from_xyz(0.0, -0.125, 0.0),
                ));
            }
        }

        // Spawn road meshes
        for road in &roads {
            if road.points.len() < 2 {
                continue;
            }

            let road_vertices: Vec<Vec3> = road
                .points
                .iter()
                .map(|point| Vec3::new(point.x, 0.0, point.y))
                .collect();

            commands.spawn((
                Mesh3d(meshes.add(Polyline3d::new(road_vertices))),
                MeshMaterial3d(road_material.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0),
            ));
        }

        // Configure a properly scaled cascade shadow map for this scene (defaults are too large, mesh units are in km)
        let cascade_shadow_config = CascadeShadowConfigBuilder {
            first_cascade_far_bound: 0.3,
            maximum_distance: 3.0,
            ..default()
        }
        .build();

        // Light
        commands.spawn((
            DirectionalLight {
                shadows_enabled: true,
                // lux::RAW_SUNLIGHT is recommended for use with this feature, since
                // other values approximate sunlight *post-scattering* in various
                // conditions. RAW_SUNLIGHT in comparison is the illuminance of the
                // sun unfiltered by the atmosphere, so it is the proper input for
                // sunlight to be filtered by the atmosphere.
                illuminance: lux::RAW_SUNLIGHT,
                ..default()
            },
            Transform::from_xyz(1.0, -0.4, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
            cascade_shadow_config,
        ));
    }
}

fn on_tile_error(mut ev_error: MessageReader<HttpResponseError>) {
    for error in ev_error.read() {
        println!("Error retrieving IP: {}", error.err);
    }
}

// Each feature.tags entry is an index into layer.keys and layer.values.
// tags are stored as pairs: [key_index, value_index, key_index, value_index, ...]
fn extract_tag_value_as_f64(tags: &Vec<u32>, layer: &Layer, input_key: String) -> Option<f64> {
    let mut output = None;
    for tag_pair in tags.chunks(2) {
        if tag_pair.len() != 2 {
            continue; // malformed
        }
        let key_idx = tag_pair[0] as usize;
        let val_idx = tag_pair[1] as usize;

        if let (Some(key), Some(val)) = (layer.keys.get(key_idx), layer.values.get(val_idx)) {
            if *key == input_key {
                output = val
                    .double_value
                    .or_else(|| val.float_value.map(|v| v as f64))
                    .or_else(|| val.int_value.map(|v| v as f64))
                    .or_else(|| val.uint_value.map(|v| v as f64))
                    .or_else(|| val.sint_value.map(|v| v as f64))
                    .or_else(|| {
                        val.string_value
                            .as_ref()
                            .and_then(|s| s.parse::<f64>().ok())
                    });
            }
        }
    }
    output
}

// --- Building mesh ---
fn create_building_mesh(building: &Building) -> Option<Mesh> {
    let outer_ring = building.geometry.first()?;
    if outer_ring.len() < 3 {
        return None;
    }

    let points: Vec<[f64; 2]> = outer_ring
        .iter()
        .map(|p| [p.x as f64, p.y as f64])
        .collect();
    let sketch: csgrs::sketch::Sketch<()> = csgrs::sketch::Sketch::polygon(&points, None);
    let height = building.height.unwrap_or(10.0);
    let extruded = sketch.extrude(height);
    let rotated = extruded.rotate(-90.0, 0.0, 0.0);
    Some(rotated.to_bevy_mesh())
}

// --- Building processor ---
struct BuildingProcessor {
    tile_x: u32,
    tile_y: u32,
    building: Option<Building>,
    current_ring: Vec<Vec2>,
    rings: Vec<Vec<Vec2>>,
}

impl BuildingProcessor {
    fn new(tile_x: u32, tile_y: u32) -> Self {
        Self {
            tile_x,
            tile_y,
            building: Some(Building {
                geometry: Vec::new(),
                height: None,
            }),
            current_ring: Vec::new(),
            rings: Vec::new(),
        }
    }

    pub fn tile_to_world_static(x: f64, y: f64, tile_x: u32, tile_y: u32, scale: f64) -> Vec2 {
        let norm_x = (tile_x as f64 + x / 4096.0) * scale;
        let norm_y = (tile_y as f64 + y / 4096.0) * scale;
        Vec2::new(norm_x as f32, norm_y as f32)
    }
}

impl GeomProcessor for BuildingProcessor {
    fn xy(&mut self, x: f64, y: f64, _idx: usize) -> geozero::error::Result<()> {
        self.current_ring.push(Self::tile_to_world_static(
            x,
            y,
            self.tile_x,
            self.tile_y,
            1000.0,
        ));
        Ok(())
    }

    fn linestring_end(&mut self, _tagged: bool, _idx: usize) -> geozero::error::Result<()> {
        if !self.current_ring.is_empty() {
            self.rings.push(self.current_ring.clone());
            self.current_ring.clear();
        }
        Ok(())
    }

    fn polygon_end(&mut self, _tagged: bool, _idx: usize) -> geozero::error::Result<()> {
        if let Some(ref mut building) = self.building {
            building.geometry = self.rings.clone();
        }
        Ok(())
    }
}

// --- Road processor ---
struct RoadProcessor {
    tile_x: u32,
    tile_y: u32,
    roads: Vec<Road>,
    current_line: Vec<Vec2>,
}

impl RoadProcessor {
    fn new(tile_x: u32, tile_y: u32) -> Self {
        Self {
            tile_x,
            tile_y,
            roads: Vec::new(),
            current_line: Vec::new(),
        }
    }

    pub fn tile_to_world_static(x: f64, y: f64, tile_x: u32, tile_y: u32) -> Vec2 {
        let scale = 1000.0;
        let norm_x = (tile_x as f64 + x / 4096.0) * scale;
        let norm_y = (tile_y as f64 + y / 4096.0) * scale;
        Vec2::new(norm_x as f32, norm_y as f32)
    }
}

impl GeomProcessor for RoadProcessor {
    fn xy(&mut self, x: f64, y: f64, _idx: usize) -> geozero::error::Result<()> {
        self.current_line
            .push(Self::tile_to_world_static(x, y, self.tile_x, self.tile_y));
        Ok(())
    }

    fn linestring_end(&mut self, _tagged: bool, _idx: usize) -> geozero::error::Result<()> {
        if !self.current_line.is_empty() {
            self.roads.push(Road {
                points: self.current_line.clone(),
            });
            self.current_line.clear();
        }
        Ok(())
    }
}
