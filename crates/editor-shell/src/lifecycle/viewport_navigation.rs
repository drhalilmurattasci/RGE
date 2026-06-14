use std::time::{Duration, Instant};

use crate::camera::EditorCameraState;

const VIEWPORT_ORBIT_RADIANS_PER_PIXEL: f32 = 0.01;
const VIEWPORT_LEFT_DOUBLE_CLICK_MAX_INTERVAL: Duration = Duration::from_millis(500);
const VIEWPORT_LEFT_DOUBLE_CLICK_MAX_DISTANCE_PX: f32 = 6.0;

#[derive(Clone, Copy, Debug)]
struct ViewportLeftClick {
    cursor_pos: [f32; 2],
    at: Instant,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ViewportLeftDoubleClick {
    previous: Option<ViewportLeftClick>,
}

impl ViewportLeftDoubleClick {
    pub(super) fn reset(&mut self) {
        self.previous = None;
    }

    pub(super) fn left_press(
        &mut self,
        cursor_pos: Option<[f32; 2]>,
        over_viewport_tab: bool,
        at: Instant,
    ) -> bool {
        if !over_viewport_tab {
            self.reset();
            return false;
        }

        let Some(cursor_pos) = cursor_pos else {
            self.reset();
            return false;
        };
        if !cursor_pos[0].is_finite() || !cursor_pos[1].is_finite() {
            self.reset();
            return false;
        }

        let is_double_click = self.previous.is_some_and(|previous| {
            let Some(elapsed) = at.checked_duration_since(previous.at) else {
                return false;
            };
            if elapsed > VIEWPORT_LEFT_DOUBLE_CLICK_MAX_INTERVAL {
                return false;
            }

            let dx = cursor_pos[0] - previous.cursor_pos[0];
            let dy = cursor_pos[1] - previous.cursor_pos[1];
            if !dx.is_finite() || !dy.is_finite() {
                return false;
            }

            dx.mul_add(dx, dy * dy)
                <= VIEWPORT_LEFT_DOUBLE_CLICK_MAX_DISTANCE_PX
                    * VIEWPORT_LEFT_DOUBLE_CLICK_MAX_DISTANCE_PX
        });

        if is_double_click {
            self.reset();
        } else {
            self.previous = Some(ViewportLeftClick { cursor_pos, at });
        }

        is_double_click
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ViewportOrbitDrag {
    last_cursor_pos: Option<[f32; 2]>,
}

impl ViewportOrbitDrag {
    pub(super) fn is_active(&self) -> bool {
        self.last_cursor_pos.is_some()
    }

    pub(super) fn start(&mut self, cursor_pos: [f32; 2]) {
        if cursor_pos[0].is_finite() && cursor_pos[1].is_finite() {
            self.last_cursor_pos = Some(cursor_pos);
        }
    }

    pub(super) fn stop(&mut self) {
        self.last_cursor_pos = None;
    }

    pub(super) fn drag_to(&mut self, cursor_pos: [f32; 2], camera: &mut EditorCameraState) {
        let Some(last_cursor_pos) = self.last_cursor_pos else {
            return;
        };
        if !cursor_pos[0].is_finite() || !cursor_pos[1].is_finite() {
            return;
        }

        let delta = [
            cursor_pos[0] - last_cursor_pos[0],
            cursor_pos[1] - last_cursor_pos[1],
        ];
        if !delta[0].is_finite() || !delta[1].is_finite() {
            return;
        }

        self.last_cursor_pos = Some(cursor_pos);
        camera.orbit_around_target(
            -delta[0] * VIEWPORT_ORBIT_RADIANS_PER_PIXEL,
            -delta[1] * VIEWPORT_ORBIT_RADIANS_PER_PIXEL,
        );
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ViewportPanDrag {
    last_cursor_pos: Option<[f32; 2]>,
}

impl ViewportPanDrag {
    pub(super) fn is_active(&self) -> bool {
        self.last_cursor_pos.is_some()
    }

    pub(super) fn start(&mut self, cursor_pos: [f32; 2]) {
        if cursor_pos[0].is_finite() && cursor_pos[1].is_finite() {
            self.last_cursor_pos = Some(cursor_pos);
        }
    }

    pub(super) fn stop(&mut self) {
        self.last_cursor_pos = None;
    }

    pub(super) fn drag_to(
        &mut self,
        cursor_pos: [f32; 2],
        viewport_size: [f32; 2],
        camera: &mut EditorCameraState,
    ) {
        let Some(last_cursor_pos) = self.last_cursor_pos else {
            return;
        };
        if !cursor_pos[0].is_finite() || !cursor_pos[1].is_finite() {
            return;
        }

        let delta = [
            cursor_pos[0] - last_cursor_pos[0],
            cursor_pos[1] - last_cursor_pos[1],
        ];
        if !delta[0].is_finite() || !delta[1].is_finite() {
            return;
        }

        if camera.pan_in_view_plane(delta, viewport_size) {
            self.last_cursor_pos = Some(cursor_pos);
        }
    }
}
