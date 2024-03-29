use crate::math::{ray_to_plane_origin, segment_to_segment};
use ecolor::Color32;
use std::ops::{Add, RangeInclusive};

use crate::shape::ShapeBuidler;
use crate::{config::PreparedGizmoConfig, gizmo::Ray, GizmoDirection, GizmoDrawData};
use glam::{DMat3, DMat4, DQuat, DVec3};

const ARROW_FADE: RangeInclusive<f64> = 0.95..=0.99;
const PLANE_FADE: RangeInclusive<f64> = 0.70..=0.86;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum TransformKind {
    Axis,
    Plane,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct PickResult {
    pub subgizmo_point: DVec3,
    pub visibility: f64,
    pub picked: bool,
    pub t: f64,
}

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum ArrowheadStyle {
    Cone,
    Square,
}

pub(crate) fn pick_arrow(
    config: &PreparedGizmoConfig,
    ray: Ray,
    direction: GizmoDirection,
) -> PickResult {
    let width = (config.scale_factor * config.visuals.stroke_width) as f64;

    let dir = gizmo_normal(config, direction);
    let start = config.translation + (dir * (width * 0.5 + inner_circle_radius(config)));

    let length = (config.scale_factor * config.visuals.gizmo_size) as f64;

    let ray_length = 1e+14;

    let (ray_t, subgizmo_t) = segment_to_segment(
        ray.origin,
        ray.origin + ray.direction * ray_length,
        start,
        start + dir * length,
    );

    let ray_point = ray.origin + ray.direction * ray_length * ray_t;
    let subgizmo_point = start + dir * length * subgizmo_t;
    let dist = (ray_point - subgizmo_point).length();

    let dot = config.eye_to_model_dir.dot(dir).abs();

    let visibility =
        (1.0 - (dot - *ARROW_FADE.start()) / (*ARROW_FADE.end() - *ARROW_FADE.start())).min(1.0);

    let picked = visibility > 0.0 && dist <= config.focus_distance as f64;

    PickResult {
        subgizmo_point,
        visibility,
        picked,
        t: ray_t,
    }
}

pub(crate) fn pick_plane(
    config: &PreparedGizmoConfig,
    ray: Ray,
    direction: GizmoDirection,
) -> PickResult {
    let origin = plane_global_origin(config, direction);

    let normal = gizmo_normal(config, direction);

    let (t, dist_from_origin) = ray_to_plane_origin(normal, origin, ray.origin, ray.direction);

    let ray_point = ray.origin + ray.direction * t;

    let dot = config
        .eye_to_model_dir
        .dot(gizmo_normal(config, direction))
        .abs();
    let visibility = (1.0
        - ((1.0 - dot) - *PLANE_FADE.start()) / (*PLANE_FADE.end() - *PLANE_FADE.start()))
    .min(1.0);

    let picked = visibility > 0.0 && dist_from_origin <= plane_size(config);

    PickResult {
        subgizmo_point: ray_point,
        visibility,
        picked,
        t,
    }
}

pub(crate) fn pick_circle(
    config: &PreparedGizmoConfig,
    ray: Ray,
    radius: f64,
    filled: bool,
) -> PickResult {
    let origin = config.translation;
    let normal = -config.view_forward();

    let (t, dist_from_gizmo_origin) =
        ray_to_plane_origin(normal, origin, ray.origin, ray.direction);

    let hit_pos = ray.origin + ray.direction * t;

    let picked = if filled {
        dist_from_gizmo_origin <= radius + config.focus_distance as f64
    } else {
        (dist_from_gizmo_origin - radius).abs() <= config.focus_distance as f64
    };

    PickResult {
        subgizmo_point: hit_pos,
        visibility: 1.0,
        picked,
        t,
    }
}

pub(crate) fn draw_arrow(
    config: &PreparedGizmoConfig,
    opacity: f32,
    focused: bool,
    direction: GizmoDirection,
    arrowhead_style: ArrowheadStyle,
) -> GizmoDrawData {
    if opacity <= 1e-4 {
        return GizmoDrawData::default();
    }

    let color = gizmo_color(config, focused, direction).gamma_multiply(opacity);

    let transform = if config.local_space() {
        DMat4::from_rotation_translation(config.rotation, config.translation)
    } else {
        DMat4::from_translation(config.translation)
    };

    let shape_builder = ShapeBuidler::new(
        config.view_projection * transform,
        config.viewport,
        config.pixels_per_point,
    );

    let direction = gizmo_local_normal(config, direction);
    let width = (config.scale_factor * config.visuals.stroke_width) as f64;
    let length = (config.scale_factor * config.visuals.gizmo_size) as f64;

    let start = direction * (width * 0.5 + inner_circle_radius(config));
    let end = direction * length;

    let mut draw_data = GizmoDrawData::default();
    draw_data = draw_data.add(
        shape_builder
            .line_segment(start, end, (config.visuals.stroke_width, color))
            .into(),
    );

    match arrowhead_style {
        ArrowheadStyle::Square => {
            let end_stroke_width = config.visuals.stroke_width * 2.5;
            let end_length = config.scale_factor * end_stroke_width;

            draw_data = draw_data.add(
                shape_builder
                    .line_segment(
                        end,
                        end + direction * end_length as f64,
                        (end_stroke_width, color),
                    )
                    .into(),
            );
        }
        ArrowheadStyle::Cone => {
            let arrow_length = width * 2.4;

            draw_data = draw_data.add(
                shape_builder
                    .arrow(
                        end,
                        end + direction * arrow_length,
                        (config.visuals.stroke_width * 1.2, color),
                    )
                    .into(),
            );
        }
    }

    draw_data
}

pub(crate) fn draw_plane(
    config: &PreparedGizmoConfig,
    opacity: f32,
    focused: bool,
    direction: GizmoDirection,
) -> GizmoDrawData {
    if opacity <= 1e-4 {
        return GizmoDrawData::default();
    }

    let color = gizmo_color(config, focused, direction).gamma_multiply(opacity);

    let transform = if config.local_space() {
        DMat4::from_rotation_translation(config.rotation, config.translation)
    } else {
        DMat4::from_translation(config.translation)
    };

    let shape_builder = ShapeBuidler::new(
        config.view_projection * transform,
        config.viewport,
        config.pixels_per_point,
    );

    let scale = plane_size(config) * 0.5;
    let a = plane_bitangent(direction) * scale;
    let b = plane_tangent(direction) * scale;
    let origin = plane_local_origin(config, direction);

    let mut draw_data = GizmoDrawData::default();
    draw_data = draw_data.add(
        shape_builder
            .polygon(
                &[
                    origin - b - a,
                    origin + b - a,
                    origin + b + a,
                    origin - b + a,
                ],
                color,
                (0.0, Color32::TRANSPARENT),
            )
            .into(),
    );
    draw_data
}

pub(crate) fn draw_circle(
    config: &PreparedGizmoConfig,
    color: Color32,
    radius: f64,
    filled: bool,
) -> GizmoDrawData {
    if color.a() == 0 {
        return GizmoDrawData::default();
    }

    let rotation = {
        let forward = config.view_forward();
        let right = config.view_right();
        let up = config.view_up();

        DQuat::from_mat3(&DMat3::from_cols(up, -forward, -right))
    };

    let transform = DMat4::from_rotation_translation(rotation, config.translation);

    let shape_builder = ShapeBuidler::new(
        config.view_projection * transform,
        config.viewport,
        config.pixels_per_point,
    );

    let mut draw_data = GizmoDrawData::default();
    if filled {
        draw_data = draw_data.add(shape_builder.filled_circle(radius, color).into());
    } else {
        draw_data = draw_data.add(
            shape_builder
                .circle(radius, (config.visuals.stroke_width, color))
                .into(),
        );
    }
    draw_data
}

pub(crate) const fn plane_bitangent(direction: GizmoDirection) -> DVec3 {
    match direction {
        GizmoDirection::X => DVec3::Y,
        GizmoDirection::Y => DVec3::Z,
        GizmoDirection::Z => DVec3::X,
        GizmoDirection::View => DVec3::ZERO, // Unused
    }
}

pub(crate) const fn plane_tangent(direction: GizmoDirection) -> DVec3 {
    match direction {
        GizmoDirection::X => DVec3::Z,
        GizmoDirection::Y => DVec3::X,
        GizmoDirection::Z => DVec3::Y,
        GizmoDirection::View => DVec3::ZERO, // Unused
    }
}

pub(crate) fn plane_size(config: &PreparedGizmoConfig) -> f64 {
    (config.scale_factor * (config.visuals.gizmo_size * 0.1 + config.visuals.stroke_width * 2.0))
        as f64
}

pub(crate) fn plane_local_origin(config: &PreparedGizmoConfig, direction: GizmoDirection) -> DVec3 {
    let offset = config.scale_factor * config.visuals.gizmo_size * 0.5;

    let a = plane_bitangent(direction);
    let b = plane_tangent(direction);
    (a + b) * offset as f64
}

pub(crate) fn plane_global_origin(
    config: &PreparedGizmoConfig,
    direction: GizmoDirection,
) -> DVec3 {
    let mut origin = plane_local_origin(config, direction);
    if config.local_space() {
        origin = config.rotation * origin;
    }
    origin + config.translation
}

/// Radius to use for inner circle subgizmos
pub(crate) fn inner_circle_radius(config: &PreparedGizmoConfig) -> f64 {
    (config.scale_factor * config.visuals.gizmo_size) as f64 * 0.2
}

/// Radius to use for outer circle subgizmos
pub(crate) fn outer_circle_radius(config: &PreparedGizmoConfig) -> f64 {
    (config.scale_factor * (config.visuals.gizmo_size + config.visuals.stroke_width + 5.0)) as f64
}

pub fn gizmo_local_normal(config: &PreparedGizmoConfig, direction: GizmoDirection) -> DVec3 {
    match direction {
        GizmoDirection::X => DVec3::X,
        GizmoDirection::Y => DVec3::Y,
        GizmoDirection::Z => DVec3::Z,
        GizmoDirection::View => -config.view_forward(),
    }
}

pub fn gizmo_normal(config: &PreparedGizmoConfig, direction: GizmoDirection) -> DVec3 {
    let mut normal = gizmo_local_normal(config, direction);

    if config.local_space() && direction != GizmoDirection::View {
        normal = config.rotation * normal;
    }

    normal
}

pub fn gizmo_color(
    config: &PreparedGizmoConfig,
    focused: bool,
    direction: GizmoDirection,
) -> Color32 {
    let color = match direction {
        GizmoDirection::X => config.visuals.x_color,
        GizmoDirection::Y => config.visuals.y_color,
        GizmoDirection::Z => config.visuals.z_color,
        GizmoDirection::View => config.visuals.s_color,
    };

    let color = if focused {
        config.visuals.highlight_color.unwrap_or(color)
    } else {
        color
    };

    let alpha = if focused {
        config.visuals.highlight_alpha
    } else {
        config.visuals.inactive_alpha
    };

    color.linear_multiply(alpha)
}
