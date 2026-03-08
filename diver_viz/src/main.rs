use bevy::{
    asset::RenderAssetUsages,
    input::mouse::MouseMotion,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};
use bevy_http_client::prelude::*;
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
                adjust_light,
                mouse_track,
                grab_mouse,
            ),
        )
        .run();
}

fn adjust_light(
    suns: Query<&mut Transform, With<DirectionalLight>>,
    gamepads: Query<&Gamepad>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    for mut tf in suns {
        for gamepad in gamepads {
            if gamepad.pressed(GamepadButton::RightTrigger)
                || keyboard_input.pressed(KeyCode::BracketRight)
            {
                tf.rotate_x(-time.delta_secs() * std::f32::consts::PI / 10.0);
            }
            if gamepad.pressed(GamepadButton::LeftTrigger)
                || keyboard_input.pressed(KeyCode::BracketLeft)
            {
                tf.rotate_x(time.delta_secs() * std::f32::consts::PI / 10.0);
            }
            if gamepad.just_pressed(GamepadButton::South) {
                info!("light tf (rotation) {:?}", tf.rotation);
            }
        }
    }
}

fn spawn_player_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d { ..default() },
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
            Text("sticks (or WASD + mouse) to move & look".to_string()),
            Text("bumpers/brackets to adjust lights".to_string()),
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
            let l_stick = gamepad.left_stick();
            let r_stick = gamepad.right_stick();
            let d_pad = gamepad.dpad();
            let kb_wasd = Vec2::new(
                if keyboard_input.pressed(KeyCode::KeyD) {
                    1.0
                } else if keyboard_input.pressed(KeyCode::KeyA) {
                    -1.0
                } else {
                    0.0
                },
                if keyboard_input.pressed(KeyCode::KeyW) {
                    1.0
                } else if keyboard_input.pressed(KeyCode::KeyS) {
                    -1.0
                } else {
                    0.0
                },
            );

            let combined_stick_magnitude = l_stick.length() + d_pad.length() + kb_wasd.length();
            if combined_stick_magnitude > 0.1 {
                let combined_movement_intent = (l_stick + d_pad + kb_wasd).normalize();
                let move_vec = combined_movement_intent * SPEED * timer.delta_secs();
                let offset = move_vec.x * cam.local_x() + move_vec.y * -1.0 * cam.local_z();
                cam.translation += offset;
            }

            if r_stick.length() > 0.1 {
                let mut cam_adjust = r_stick;
                cam_adjust.x *= CAM_SENSITIVITY_X;
                cam_adjust.y *= CAM_SENSITIVITY_Y;
                cam.rotate_y(-1.0 * cam_adjust.x * timer.delta_secs());
                cam.rotate_local_x(cam_adjust.y * timer.delta_secs());
            }

            if gamepad.just_pressed(GamepadButton::South) {
                info!("camera tf {:?}", cam);
            }
        }
    }
}

const MOUSE_SENSITIVITY_X: f32 = 0.2;
const MOUSE_SENSITIVITY_Y: f32 = 0.1;

fn mouse_track(
    camera_transform: Query<&mut Transform, With<Camera3d>>,
    cursor_options: Single<&CursorOptions>,
    timer: Res<Time>,
    mut mouse_motion_reader: MessageReader<MouseMotion>,
) {
    if cursor_options.grab_mode == CursorGrabMode::Locked {
        for mut cam in camera_transform {
            for mouse_motion in mouse_motion_reader.read() {
                if mouse_motion.delta.length() > 0.01 {
                    let cam_adjust = Vec2::new(
                        mouse_motion.delta.x * MOUSE_SENSITIVITY_X,
                        mouse_motion.delta.y * MOUSE_SENSITIVITY_Y * -1.0,
                    );
                    cam.rotate_y(-1.0 * cam_adjust.x * timer.delta_secs());
                    cam.rotate_local_x(cam_adjust.y * timer.delta_secs());
                }
            }
        }
    }
}

fn grab_mouse(
    mut cursor_options: Single<&mut CursorOptions>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    let click = mouse.just_released(MouseButton::Left);
    if click && cursor_options.grab_mode == CursorGrabMode::None {
        cursor_options.visible = false;
        cursor_options.grab_mode = CursorGrabMode::Locked;
    } else if click {
        cursor_options.visible = true;
        cursor_options.grab_mode = CursorGrabMode::None;
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
        if let Ok(tile) = Tile::decode(bytes) {
            let mut buildings = Vec::new();
            let mut landuse = Vec::new();
            let mut roads = Vec::new();

            for layer in &tile.layers {
                if layer.name == "buildings" {
                    info!("Processing buildings layer...");
                    for feature in &layer.features {
                        let mut processor = BuildingProcessor::new(TILE_COORD_X, TILE_COORD_Y);
                        let height: Option<f64> =
                            extract_tag_value_as_f64(&feature.tags, layer, "height".to_string());
                        if geozero::mvt::process_geom(feature, &mut processor).is_ok() {
                            if let Some(mut building) = processor.building {
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
                        let mut processor =
                            RoadProcessor::new(TILE_COORD_X, TILE_COORD_Y, width, kind);
                        if geozero::mvt::process_geom(feature, &mut processor).is_ok() {
                            roads.extend(processor.roads);
                        }
                    }
                } else if layer.name == "landuse" {
                    info!("Processing landuse layer...");
                    for feature in &layer.features {
                        let kind: String =
                            extract_tag_value_as_string(&feature.tags, layer, "kind".to_string())
                                .unwrap_or("other".to_string());
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

            // Apply center offset
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
                    .map(|v2| coord! { x: v2.x, y: v2.y })
                    .collect();
                let road_vertices_2d = LineString::new(coords);
                let buff_road: MultiPolygon<f32> = road_vertices_2d.buffer(road.width / 2.0);

                let road_height = 0.15_f32;
                for polygon in buff_road {
                    let points: Vec<Vec2> = polygon
                        .exterior()
                        .points()
                        .map(|p| Vec2::new(p.0.x, p.0.y))
                        .collect();
                    if let Some(mesh) = extrude_polygon_mesh(&points, road_height) {
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
        } else {
            info!("failed to parse tiles");
        }
    }
}

fn on_tile_error(mut ev_error: MessageReader<HttpResponseError>) {
    for error in ev_error.read() {
        println!("Error retrieving IP: {}", error.err);
    }
}

fn extract_tag_value_as_f64(tags: &Vec<u32>, layer: &Layer, input_key: String) -> Option<f64> {
    let mut output = None;
    for tag_pair in tags.chunks(2) {
        if tag_pair.len() != 2 {
            continue;
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
            continue;
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

// ---------------------------------------------------------------------------
// Mesh generation — replaces csgrs extrude + rotate + to_bevy_mesh
// ---------------------------------------------------------------------------

/// Triangulate a simple polygon (no holes) using the ear-clipping algorithm.
/// Input: 2D vertices in order (CW or CCW).
/// Returns: indices into the input slice, as triangles.
fn triangulate_polygon(verts: &[Vec2]) -> Vec<usize> {
    let n = verts.len();
    if n < 3 {
        return vec![];
    }

    // Work with an index list we can shrink as ears are clipped.
    let mut indices: Vec<usize> = (0..n).collect();
    let mut result = Vec::new();

    // Signed area — tells us winding order so we can orient consistently.
    let signed_area: f32 = {
        let mut s = 0.0_f32;
        for i in 0..n {
            let j = (i + 1) % n;
            s += verts[i].x * verts[j].y - verts[j].x * verts[i].y;
        }
        s * 0.5
    };
    // We want CCW winding for the "is point inside triangle" test below.
    // If area is negative (CW), reverse the index list.
    if signed_area < 0.0 {
        indices.reverse();
    }

    // Returns true if point P is strictly inside triangle ABC (all CCW).
    let point_in_triangle = |a: Vec2, b: Vec2, c: Vec2, p: Vec2| -> bool {
        let cross = |o: Vec2, u: Vec2, v: Vec2| (u - o).perp_dot(v - o);
        cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
    };

    // Returns true if the vertex at position `i` in the current index list
    // is a convex (ear) vertex.
    let is_ear = |indices: &[usize], i: usize| -> bool {
        let len = indices.len();
        let prev = indices[(i + len - 1) % len];
        let curr = indices[i];
        let next = indices[(i + 1) % len];
        let a = verts[prev];
        let b = verts[curr];
        let c = verts[next];
        // Must be convex (left turn in CCW polygon).
        if (b - a).perp_dot(c - a) <= 0.0 {
            return false;
        }
        // No other vertex may lie inside this triangle.
        for (_j, &idx) in indices.iter().enumerate() {
            if idx == prev || idx == curr || idx == next {
                continue;
            }
            if point_in_triangle(a, b, c, verts[idx]) {
                return false;
            }
        }
        true
    };

    // Ear-clip loop.
    let mut remaining = indices.clone();
    let mut guard = 0;
    while remaining.len() > 3 {
        guard += 1;
        if guard > remaining.len() * remaining.len() + 10 {
            // Degenerate polygon — bail out with what we have.
            break;
        }
        let len = remaining.len();
        let mut clipped = false;
        for i in 0..len {
            if is_ear(&remaining, i) {
                let prev = remaining[(i + len - 1) % len];
                let curr = remaining[i];
                let next = remaining[(i + 1) % len];
                result.push(prev);
                result.push(curr);
                result.push(next);
                remaining.remove(i);
                clipped = true;
                guard = 0;
                break;
            }
        }
        if !clipped {
            break; // Give up on degenerate input.
        }
    }
    if remaining.len() == 3 {
        result.push(remaining[0]);
        result.push(remaining[1]);
        result.push(remaining[2]);
    }
    result
}

/// Build a Bevy `Mesh` from a 2-D polygon outline + extrusion height.
///
/// csgrs used:
///   `Sketch::polygon(&points).extrude(h).rotate(-90, 0, 0).to_bevy_mesh()`
///
/// That pipeline:
///   1. Treats the 2-D polygon as lying in the XY plane.
///   2. Extrudes it along +Z, creating a prism with height `h`.
///   3. Rotates -90 ° around X → Z becomes -Y, so the prism now stands
///      upright in Bevy's Y-up world with the base at Y=0 and the top at Y=h.
///
/// We replicate the same geometry directly, in world (Y-up) space:
///   • bottom cap at Y = 0
///   • top    cap at Y = h
///   • side walls connecting the two
///
/// The polygon ring is assumed to have no holes (the original code only ever
/// passes `building.geometry.first()` / the exterior ring of a buffered road).
pub fn extrude_polygon_mesh(ring: &[Vec2], height: f32) -> Option<Mesh> {
    let n = ring.len();
    if n < 3 {
        return None;
    }

    // Remove the duplicate closing vertex that geo / OSM data often appends.
    let ring: Vec<Vec2> = if ring.first() == ring.last() && n > 3 {
        ring[..n - 1].to_vec()
    } else {
        ring.to_vec()
    };
    let n = ring.len();

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut tri_indices: Vec<u32> = Vec::new();

    // -----------------------------------------------------------------------
    // Top cap  (Y = height, normal = +Y)
    // -----------------------------------------------------------------------
    let top_base = positions.len() as u32;
    for v in &ring {
        positions.push([v.x, height, v.y]);
        normals.push([0.0, 1.0, 0.0]);
    }
    // Ear-clip the top face. csgrs rotated after extruding so the "top" of
    // the prism (originally the cap in the +Z direction) ends up at +Y.
    let top_tris = triangulate_polygon(&ring);
    for idx in top_tris {
        tri_indices.push(top_base + idx as u32);
    }

    // -----------------------------------------------------------------------
    // Bottom cap  (Y = 0, normal = -Y, winding reversed for back-face)
    // -----------------------------------------------------------------------
    let bot_base = positions.len() as u32;
    for v in &ring {
        positions.push([v.x, 0.0, v.y]);
        normals.push([0.0, -1.0, 0.0]);
    }
    let bot_tris = triangulate_polygon(&ring);
    // Reverse winding so the normal faces downward (outward).
    for chunk in bot_tris.chunks(3) {
        tri_indices.push(bot_base + chunk[0] as u32);
        tri_indices.push(bot_base + chunk[2] as u32); // swapped
        tri_indices.push(bot_base + chunk[1] as u32);
    }

    // -----------------------------------------------------------------------
    // Side walls — one quad (two triangles) per edge
    // -----------------------------------------------------------------------
    for i in 0..n {
        let j = (i + 1) % n;
        let p0 = ring[i]; // bottom-left  of this wall quad
        let p1 = ring[j]; // bottom-right

        // Outward normal: perpendicular to the edge in the XZ plane.
        let edge = Vec2::new(p1.x - p0.x, p1.y - p0.y);
        let normal_xz = Vec2::new(edge.y, -edge.x).normalize_or_zero();
        let norm = [normal_xz.x, 0.0, normal_xz.y];

        let wall_base = positions.len() as u32;

        // 4 vertices: bottom-left, bottom-right, top-right, top-left
        positions.push([p0.x, 0.0, p0.y]); // 0 – bottom-left
        positions.push([p1.x, 0.0, p1.y]); // 1 – bottom-right
        positions.push([p1.x, height, p1.y]); // 2 – top-right
        positions.push([p0.x, height, p0.y]); // 3 – top-left

        for _ in 0..4 {
            normals.push(norm);
        }

        // Two triangles (CCW when viewed from outside):
        //   0, 1, 2  and  0, 2, 3
        tri_indices.push(wall_base);
        tri_indices.push(wall_base + 1);
        tri_indices.push(wall_base + 2);

        tri_indices.push(wall_base);
        tri_indices.push(wall_base + 2);
        tri_indices.push(wall_base + 3);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(tri_indices));
    Some(mesh)
}

// ---------------------------------------------------------------------------
// create_building_mesh — thin wrapper that keeps the call-sites unchanged
// ---------------------------------------------------------------------------
fn create_building_mesh(building: &Building) -> Option<Mesh> {
    let outer_ring = building.geometry.first()?;
    if outer_ring.len() < 3 {
        return None;
    }
    let height = building.height.unwrap_or(10.0) as f32;
    extrude_polygon_mesh(outer_ring, height)
}

// ---------------------------------------------------------------------------
// BuildingProcessor
// ---------------------------------------------------------------------------
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

// ---------------------------------------------------------------------------
// RoadProcessor
// ---------------------------------------------------------------------------
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
