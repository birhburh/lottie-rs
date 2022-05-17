use flo_curves::bezier::{curve_intersects_line, Curve};
use flo_curves::{BezierCurveFactory, Coord2};
use glam::{Mat4, Vec3};
use lottie_ast::{Animated, Bezier, KeyFrame, Rgb, Transform, Vector2D};

pub trait AnimatedExt {
    type Target;
    fn initial_value(&self) -> Self::Target;
    fn value(&self, frame: u32) -> Self::Target;
    fn is_animated(&self) -> bool;
}

impl<T> AnimatedExt for Animated<T>
where
    T: Clone + Lerp<Target = T>,
{
    type Target = T;

    fn initial_value(&self) -> Self::Target {
        self.keyframes[0].value.clone()
    }

    fn value(&self, mut frame: u32) -> Self::Target {
        if !self.is_animated() {
            return self.initial_value();
        }
        let len = self.keyframes.len() - 1;
        if let Some(window) = self.keyframes.windows(2).find(|window| {
            frame >= window[0].start_frame.unwrap() && frame < window[1].start_frame.unwrap()
        }) {
            let p0 = &window[0];
            let p1 = &window[1];
            let ease_out = p0.easing_out.clone().unwrap();
            let ease_in = p0.easing_in.clone().unwrap();
            let frames = p1.start_frame.unwrap() - p0.start_frame.unwrap();
            let x = (frame - p0.start_frame.unwrap()) as f32 / frames as f32;
            debug_assert!(x <= 1.0 && x >= 0.0);
            let curve = Curve::from_points(
                Coord2(0.0, 0.0),
                (
                    Coord2(ease_out.x[0] as f64, ease_out.y[0] as f64),
                    Coord2(ease_in.x[0] as f64, ease_in.y[0] as f64),
                ),
                Coord2(1.0, 1.0),
            );
            let intersection =
                curve_intersects_line(&curve, &(Coord2(x as f64, 0.0), Coord2(x as f64, 1.0)));
            let ratio = if intersection.is_empty() {
                x
            } else {
                intersection[0].2 .1 as f32
            };
            p1.value.lerp(&p0.value, ratio)
        } else if frame >= self.keyframes[len].start_frame.unwrap_or(0) {
            self.keyframes[len].value.clone()
        } else {
            self.keyframes[0].value.clone()
        }
    }

    fn is_animated(&self) -> bool {
        self.animated
    }
}

impl AnimatedExt for Transform {
    type Target = Mat4;

    fn initial_value(&self) -> Self::Target {
        self.value(0)
    }

    fn value(&self, frame: u32) -> Self::Target {
        let mut angle = 0.0;
        if let Some(position) = self.position.as_ref() && self.auto_orient {
            if position.is_animated() {
                let len = position.keyframes.len() - 1;
                let mut frame =
                    std::cmp::max(position.keyframes[0].start_frame.unwrap_or(0), frame);
                frame = std::cmp::min(position.keyframes[len].start_frame.unwrap_or(0), frame);
                if let Some(window) = position.keyframes.windows(2).find(|window| {
                    frame >= window[0].start_frame.unwrap()
                        && frame < window[1].start_frame.unwrap()
                }) {
                    let p0 = &window[0];
                    let p1 = &window[1];
                    angle = (p1.value - p0.value).angle_from_x_axis().to_degrees();
                }
            }
        }
        let anchor = self
            .anchor
            .as_ref()
            .map(|a| a.value(frame))
            .unwrap_or_default();
        let position = self
            .position
            .as_ref()
            .map(|a| a.value(frame))
            .unwrap_or_default();
        let scale = self.scale.value(frame) / 100.0;
        let rotation = self.rotation.value(frame) + angle;
        mat4(anchor, position, scale, rotation)
    }

    fn is_animated(&self) -> bool {
        self.anchor
            .as_ref()
            .map(|a| a.is_animated())
            .unwrap_or(false)
            || self
                .position
                .as_ref()
                .map(|a| a.is_animated())
                .unwrap_or(false)
            || self.scale.is_animated()
            || self.rotation.is_animated()
    }
}

fn mat4(anchor: Vector2D, position: Vector2D, scale: Vector2D, rotation: f32) -> Mat4 {
    let anchor = Vec3::new(anchor.x, anchor.y, 0.0);
    let scale = Vec3::new(scale.x, scale.y, 1.0);
    let position = Vec3::new(position.x, position.y, 0.0);
    Mat4::from_translation(position)
        * Mat4::from_rotation_z(rotation * std::f32::consts::PI / 180.0)
        * Mat4::from_scale(scale)
        * Mat4::from_translation(-anchor)
}

pub trait KeyFrameExt {
    fn alter_value<U>(&self, value: U) -> KeyFrame<U>;
}

impl<T> KeyFrameExt for KeyFrame<T> {
    fn alter_value<U>(&self, value: U) -> KeyFrame<U> {
        KeyFrame {
            value,
            start_frame: self.start_frame.clone(),
            easing_out: self.easing_out.clone(),
            easing_in: self.easing_in.clone(),
        }
    }
}

pub trait Lerp {
    type Target;
    fn lerp(&self, other: &Self, t: f32) -> Self::Target;
}

impl Lerp for Vector2D {
    type Target = Vector2D;

    fn lerp(&self, other: &Self, t: f32) -> Self::Target {
        (*self - *other) * t + other
    }
}

impl Lerp for f32 {
    type Target = f32;

    fn lerp(&self, other: &Self, t: f32) -> Self::Target {
        (*self - *other) * t + *other
    }
}

impl Lerp for Vec<Bezier> {
    type Target = Vec<Bezier>;

    fn lerp(&self, other: &Self, t: f32) -> Self::Target {
        let mut result = self.clone();
        for (bezier, other) in result.iter_mut().zip(other.iter()) {
            for (v, other_v) in bezier.verticies.iter_mut().zip(other.verticies.iter()) {
                *v = v.lerp(*other_v, t);
            }
            for (v, other_v) in bezier.in_tangent.iter_mut().zip(other.in_tangent.iter()) {
                *v = v.lerp(*other_v, t);
            }
            for (v, other_v) in bezier.out_tangent.iter_mut().zip(other.out_tangent.iter()) {
                *v = v.lerp(*other_v, t);
            }
        }
        result
    }
}

impl Lerp for Rgb {
    type Target = Rgb;

    fn lerp(&self, other: &Self, t: f32) -> Self::Target {
        todo!()
    }
}
