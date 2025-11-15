use bevy::prelude::*;
use bevy_http_client::prelude::*;
use csgrs::traits::CSG;
use geo::{Buffer, Coord, LineString, MultiPolygon, coord};
use geozero::GeomProcessor;
use geozero::mvt::tile::Layer;
use geozero::mvt::{Message, Tile};

const MARTIN_MVT_ENDPOINT: &str =
    "https://denver.roboape.online/tiles/denver_blocks_all_zoom_15_up";

#[derive(Debug, Clone)]
struct Building {
    geometry: Vec<Vec<Vec2>>,
    height: Option<f64>,
    color: Option<Color>,
}

#[derive(Debug, Clone)]
struct Road {
    points: Vec<Vec2>,
    width: f32,
    kind: String,
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
        .add_systems(Startup, (spawn_player_camera, spawn_ui_text, request_tiles))
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
    suns: Query<&mut Transform, With<DirectionalLight>>,
    gamepads: Query<&Gamepad>,
    time: Res<Time>,
) {
    for mut tf in suns {
        for gamepad in gamepads {
            // debug light adjustments
            if gamepad.pressed(GamepadButton::RightTrigger) {
                tf.rotate_x(-time.delta_secs() * std::f32::consts::PI / 10.0);
            }
            if gamepad.pressed(GamepadButton::LeftTrigger) {
                tf.rotate_x(time.delta_secs() * std::f32::consts::PI / 10.0);
            }
            // debug print trigger
            if gamepad.just_pressed(GamepadButton::South) {
                // print out current directional light rotations
                info!("light tf (rotation) {:?}", tf.rotation);
            }
        }
    }
}

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d { ..default() },
        // Picked a value that looked nice looking at the capitol
        Transform::from_xyz(58.50679, 4.5122952, 78.189224).with_rotation(Quat::from_xyzw(
            0.07673687,
            0.50015175,
            -0.04455679,
            0.8613791,
        )),
        DistanceFog {
            color: Color::srgb(0.50, 0.44, 0.63),
            falloff: FogFalloff::ExponentialSquared { density: 0.01 },
            ..default()
        },
    ));
}

fn spawn_ui_text(mut commands: Commands) {
    commands.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            position_type: PositionType::Absolute,
            row_gap: px(6),
            left: px(12),
            top: px(12),
            ..default()
        },
        children![
            Text("left/right stick to move".to_string()),
            Text("bumpers to adjust lights".to_string()),
        ],
    ));

    commands.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            position_type: PositionType::Absolute,
            row_gap: px(6),
            right: px(12),
            bottom: px(12),
            ..default()
        },
        children![(
            Text("map data from OpenStreetMap".to_string()),
            TextFont {
                font_size: 12.0,
                ..Default::default()
            },
        )],
    ));
}

const CAM_SENSITIVITY_X: f32 = 1.1;
const CAM_SENSITIVITY_Y: f32 = 0.7;
const SPEED: f32 = 12.0;

fn camera_update(
    camera_transform: Query<&mut Transform, With<Camera3d>>,
    gamepads: Query<&Gamepad>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    timer: Res<Time>,
) {
    for mut cam in camera_transform {
        for gamepad in gamepads {
            // get unscaled stick inputs + dpad
            let l_stick = gamepad.left_stick();
            let r_stick = gamepad.right_stick();
            let d_pad = gamepad.dpad();
            // also keyboard i guess *eyeroll*
            let kb_wasd = Vec2::new(
                if keyboard_input.pressed(KeyCode::KeyD) { 1.0 } else if keyboard_input.pressed(KeyCode::KeyA) { -1.0 } else {0.0},
                if keyboard_input.pressed(KeyCode::KeyW) { 1.0 } else if keyboard_input.pressed(KeyCode::KeyS) { -1.0 } else {0.0},
            );

            // movement
            let combined_stick_input = (l_stick + d_pad + kb_wasd).normalize();
            if combined_stick_input.length() > 0.1 {
                let move_vec = combined_stick_input * SPEED
                    * timer.delta_secs();

                let offset = move_vec.x * cam.local_x() + move_vec.y * -1.0 * cam.local_z();
                cam.translation += offset;
            }

            // camera
            if r_stick.length() > 0.1 {
                let mut cam_adjust = r_stick;
                cam_adjust.x *= CAM_SENSITIVITY_X;
                cam_adjust.y *= CAM_SENSITIVITY_Y;
                cam.rotate_y(-1.0 * cam_adjust.x * timer.delta_secs());
                cam.rotate_local_x(cam_adjust.y * timer.delta_secs());
            }

            // debug prints
            if gamepad.just_pressed(GamepadButton::South) {
                // print out current camera tf
                info!("camera tf {:?}", cam);
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
                    let kind: String =
                        extract_tag_value_as_string(&feature.tags, layer, "kind".to_string())
                            .unwrap_or("other".to_string());
                    let width = match kind.as_str() {
                        "major_road" => 4.0,
                        "minor_road" => 0.125,
                        "path" => 0.06,
                        _ => 0.02,
                    };
                    let mut processor = RoadProcessor::new(TILE_COORD_X, TILE_COORD_Y, width, kind);
                    if geozero::mvt::process_geom(feature, &mut processor).is_ok() {
                        roads.extend(processor.roads);
                    }
                }
            } else if layer.name == "landuse" {
                // NOTE: This just uses the same processing as buildings for now.
                // Should read the tags and do some different stuff here for diff types
                info!("Processing landuse layer...");
                for feature in &layer.features {
                    let kind: String =
                        extract_tag_value_as_string(&feature.tags, layer, "kind".to_string())
                            .unwrap_or("other".to_string());
                    // switch up the height based on the 'kind' of landuse polygon
                    let height = match kind.as_str() {
                        "other" => 0.05,
                        "grass" => 0.08,
                        "pedestrian" => 0.1,
                        _ => 0.02,
                    };
                    let color = match kind.as_str() {
                        "other" => Color::srgb(0.9, 0.58, 0.43),
                        "grass" => Color::srgb(0.25, 0.58, 0.43),
                        "pedestrian" => Color::srgb(0.62, 0.67, 0.60),
                        _ => Color::srgb(0.85, 0.04, 0.30),
                    };
                    let mut processor = BuildingProcessor::new(TILE_COORD_X, TILE_COORD_Y);
                    if geozero::mvt::process_geom(feature, &mut processor).is_ok() {
                        if let Some(mut building) = processor.building {
                            // add hardcoded height for now
                            building.height = Some(height);
                            building.color = Some(color);
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

        let major_road_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.98, 0.37, 0.43),
            metallic: 1.0,
            perceptual_roughness: 0.0,
            ..default()
        });
        let minor_road_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.88, 0.41, 0.63),
            metallic: 1.0,
            perceptual_roughness: 0.0,
            ..default()
        });
        let other_road_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.78, 0.37, 0.93),
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
                let landuse_material = materials.add(StandardMaterial {
                    base_color: landuse_poly.color.unwrap(),
                    metallic: 0.0,
                    perceptual_roughness: 1.0,
                    ..default()
                });
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
            let coords: Vec<Coord<f32>> = road
                .points
                .iter()
                .map(|v2| {
                    coord! { x: v2.x, y: v2.y }
                })
                .collect();
            // make road verts a geo linestring
            let road_vertices_2d = LineString::new(coords);
            // buffer it based on width
            let buff_road: MultiPolygon<f32> = road_vertices_2d.buffer(road.width / 2.0);

            let road_height = 0.15;
            // take the resulting polygons and make em into extruded meshes
            for polygon in buff_road {
                let points: Vec<[f64; 2]> = polygon
                    .exterior()
                    .points()
                    .map(|p| [p.0.x as f64, p.0.y as f64])
                    .collect();
                let sketch: csgrs::sketch::Sketch<()> =
                    csgrs::sketch::Sketch::polygon(&points, None);
                let extruded = sketch.extrude(road_height);
                let rotated = extruded.rotate(-90.0, 0.0, 0.0);
                let mesh = rotated.to_bevy_mesh();

                commands.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(match road.kind.as_str() {
                        "major_road" => major_road_material.clone(),
                        "minor_road" => minor_road_material.clone(),
                        _ => other_road_material.clone(),
                    }),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                ));
            }
        }

        // Light
        commands.spawn((
            DirectionalLight {
                shadows_enabled: true,
                illuminance: 50000.0,
                ..default()
            },
            Transform::from_xyz(1.0, -0.4, 0.0).with_rotation(Quat::from_xyzw(
                -0.6469852,
                0.02463232,
                -0.70667696,
                0.285324,
            )),
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

fn extract_tag_value_as_string(
    tags: &Vec<u32>,
    layer: &Layer,
    input_key: String,
) -> Option<String> {
    let mut output = None;
    for tag_pair in tags.chunks(2) {
        if tag_pair.len() != 2 {
            continue; // malformed
        }
        let key_idx = tag_pair[0] as usize;
        let val_idx = tag_pair[1] as usize;

        if let (Some(key), Some(val)) = (layer.keys.get(key_idx), layer.values.get(val_idx)) {
            if *key == input_key {
                output = val.string_value.clone()
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
                color: None,
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
    width: f32,
    kind: String,
}

impl RoadProcessor {
    fn new(tile_x: u32, tile_y: u32, width: f32, kind: String) -> Self {
        Self {
            tile_x,
            tile_y,
            roads: Vec::new(),
            current_line: Vec::new(),
            width,
            kind,
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
                width: self.width,
                kind: self.kind.clone(),
            });
            self.current_line.clear();
        }
        Ok(())
    }
}
