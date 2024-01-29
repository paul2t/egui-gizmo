use std::f64::consts::{FRAC_PI_2, PI, TAU};

use egui::Ui;
use glam::{DMat3, DMat4, DQuat, DVec2, DVec3};

use crate::math::{ray_to_plane_origin, rotation_align, round_to_interval, world_to_screen};
use crate::painter::Painter3d;
use crate::subgizmo::{SubGizmo, SubGizmoConfig, SubGizmoState};
use crate::{GizmoDirection, GizmoMode, GizmoResult, Ray};

pub(crate) type RotationSubGizmo = SubGizmoConfig<RotationState>;

impl SubGizmo for RotationSubGizmo {
    fn pick(&mut self, ui: &Ui, ray: Ray) -> Option<f64> {
        let radius = arc_radius(self) as f64;
        let config = self.config;
        let origin = config.translation;
        let normal = self.normal();
        let tangent = tangent(self);

        let (t, dist_from_gizmo_origin) =
            ray_to_plane_origin(normal, origin, ray.origin, ray.direction);
        let dist_from_gizmo_edge = (dist_from_gizmo_origin - radius).abs();

        let hit_pos = ray.origin + ray.direction * t;
        let dir_to_origin = (origin - hit_pos).normalize();
        let nearest_circle_pos = hit_pos + dir_to_origin * (dist_from_gizmo_origin - radius);

        let offset = (nearest_circle_pos - origin).normalize();

        let angle = if self.direction == GizmoDirection::Screen {
            f64::atan2(tangent.cross(normal).dot(offset), tangent.dot(offset))
        } else {
            let mut forward = config.view_forward();
            if config.left_handed {
                forward *= -1.0;
            }
            f64::atan2(offset.cross(forward).dot(normal), offset.dot(forward))
        };

        self.update_state_with(ui, |state: &mut RotationState| {
            let rotation_angle = rotation_angle(self, ui).unwrap_or(0.0);
            state.start_axis_angle = angle as f32;
            state.start_rotation_angle = rotation_angle as f32;
            state.last_rotation_angle = rotation_angle as f32;
            state.current_delta = 0.0;
        });

        if dist_from_gizmo_edge <= config.focus_distance as f64 && angle.abs() < arc_angle(self) {
            Some(t)
        } else {
            None
        }
    }

    fn update(&mut self, ui: &Ui, _ray: Ray) -> Option<GizmoResult> {
        let state = self.state(ui);
        let config = self.config;

        let mut rotation_angle = rotation_angle(self, ui)?;
        if config.snapping {
            rotation_angle = round_to_interval(
                rotation_angle - state.start_rotation_angle as f64,
                config.snap_angle as f64,
            ) + state.start_rotation_angle as f64;
        }

        let mut angle_delta = rotation_angle - state.last_rotation_angle as f64;

        // Always take the smallest angle, e.g. -10° instead of 350°
        if angle_delta > PI {
            angle_delta -= TAU;
        } else if angle_delta < -PI {
            angle_delta += TAU;
        }

        self.update_state_with(ui, |state: &mut RotationState| {
            state.last_rotation_angle = rotation_angle as f32;
            state.current_delta += angle_delta as f32;
        });

        let new_rotation =
            DQuat::from_axis_angle(self.normal(), -angle_delta) * self.config.rotation;

        Some(GizmoResult {
            scale: self.config.scale.as_vec3().into(),
            rotation: new_rotation.as_f32().into(),
            translation: self.config.translation.as_vec3().into(),
            mode: GizmoMode::Rotate,
            value: (self.normal().as_vec3() * state.current_delta).to_array(),
        })
    }

    fn draw(&self, ui: &Ui) {
        let state = self.state(ui);
        let config = self.config;

        let transform = rotation_matrix(self);
        let painter = Painter3d::new(
            ui.painter().clone(),
            config.view_projection * transform,
            config.viewport,
        );

        let color = self.color();
        let stroke = (config.visuals.stroke_width, color);

        let radius = arc_radius(self) as f64;

        if !self.active {
            let angle = arc_angle(self);
            painter.arc(radius, FRAC_PI_2 - angle, FRAC_PI_2 + angle, stroke);
        } else {
            let start_angle = state.start_axis_angle as f64 + FRAC_PI_2;
            let end_angle = start_angle + state.current_delta as f64;

            // The polyline does not get rendered correctly if
            // the start and end lines are exactly the same
            let end_angle = end_angle + 1e-5;

            painter.polyline(
                &[
                    DVec3::new(start_angle.cos() * radius, 0.0, start_angle.sin() * radius),
                    DVec3::new(0.0, 0.0, 0.0),
                    DVec3::new(end_angle.cos() * radius, 0.0, end_angle.sin() * radius),
                ],
                stroke,
            );

            painter.circle(radius, stroke);

            // Draw snapping ticks
            if config.snapping {
                let stroke_width = stroke.0 / 2.0;
                for i in 0..((TAU / config.snap_angle as f64) as usize + 1) {
                    let angle = i as f64 * config.snap_angle as f64 + end_angle;
                    let pos = DVec3::new(angle.cos(), 0.0, angle.sin());
                    painter.line_segment(
                        pos * radius * 1.1,
                        pos * radius * 1.2,
                        (stroke_width, stroke.1),
                    );
                }
            }
        }
    }
}

/// Calculates angle of the rotation axis arc.
/// The arc is a semicircle, which turns into a full circle when viewed
/// directly from the front.
fn arc_angle(subgizmo: &SubGizmoConfig<RotationState>) -> f64 {
    let dot = subgizmo.normal().dot(subgizmo.config.view_forward()).abs();
    let min_dot = 0.990;
    let max_dot = 0.995;

    let mut angle =
        f64::min(1.0, f64::max(0.0, dot - min_dot) / (max_dot - min_dot)) * FRAC_PI_2 + FRAC_PI_2;
    if (angle - PI).abs() < 1e-2 {
        angle = PI;
    }
    angle
}

/// Calculates a matrix used when rendering the rotation axis.
fn rotation_matrix(subgizmo: &SubGizmoConfig<RotationState>) -> DMat4 {
    if subgizmo.direction == GizmoDirection::Screen {
        let forward = subgizmo.config.view_forward();
        let right = subgizmo.config.view_right();
        let up = subgizmo.config.view_up();

        let rotation = DQuat::from_mat3(&DMat3::from_cols(up, -forward, -right));

        return DMat4::from_rotation_translation(rotation, subgizmo.config.translation);
    }

    // First rotate towards the gizmo normal
    let local_normal = subgizmo.local_normal();
    let rotation = rotation_align(DVec3::Y, local_normal);
    let mut rotation = DQuat::from_mat3(&rotation);
    let config = subgizmo.config;

    if config.local_space() {
        rotation = config.rotation * rotation;
    }

    let tangent = tangent(subgizmo);
    let normal = subgizmo.normal();
    let mut forward = config.view_forward();
    if config.left_handed {
        forward *= -1.0;
    }
    let angle = f64::atan2(tangent.cross(forward).dot(normal), tangent.dot(forward));

    // Rotate towards the camera, along the rotation axis.
    rotation = DQuat::from_axis_angle(normal, angle) * rotation;

    DMat4::from_rotation_translation(rotation, config.translation)
}

fn rotation_angle(subgizmo: &SubGizmoConfig<RotationState>, ui: &Ui) -> Option<f64> {
    let cursor_pos = ui.input(|i| i.pointer.hover_pos())?;
    let viewport = subgizmo.config.viewport;
    let gizmo_pos = world_to_screen(viewport, subgizmo.config.mvp, DVec3::new(0.0, 0.0, 0.0))?;
    let delta = DVec2::new(
        cursor_pos.x as f64 - gizmo_pos.x as f64,
        cursor_pos.y as f64 - gizmo_pos.y as f64,
    )
    .normalize();

    if delta.is_nan() {
        return None;
    }

    let mut angle = f64::atan2(delta.y, delta.x);
    if subgizmo.config.view_forward().dot(subgizmo.normal()) < 0.0 {
        angle *= -1.0;
    }

    Some(angle)
}

fn tangent(subgizmo: &SubGizmoConfig<RotationState>) -> DVec3 {
    let mut tangent = match subgizmo.direction {
        GizmoDirection::X | GizmoDirection::Y => DVec3::Z,
        GizmoDirection::Z => -DVec3::Y,
        GizmoDirection::Screen => -subgizmo.config.view_right(),
    };

    if subgizmo.config.local_space() && subgizmo.direction != GizmoDirection::Screen {
        tangent = subgizmo.config.rotation * tangent;
    }

    tangent
}

fn arc_radius(subgizmo: &SubGizmoConfig<RotationState>) -> f32 {
    let mut radius = subgizmo.config.visuals.gizmo_size;

    if subgizmo.direction == GizmoDirection::Screen {
        // Screen axis should be a little bit larger
        radius += subgizmo.config.visuals.stroke_width + 5.0;
    }

    subgizmo.config.scale_factor * radius
}

#[derive(Default, Debug, Copy, Clone)]
pub(crate) struct RotationState {
    start_axis_angle: f32,
    start_rotation_angle: f32,
    last_rotation_angle: f32,
    current_delta: f32,
}

impl SubGizmoState for RotationState {}