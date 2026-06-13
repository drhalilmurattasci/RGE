use crate::camera::EditorCameraState;

const VIEWPORT_ORBIT_RADIANS_PER_PIXEL: f32 = 0.01;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ViewportOrbitDrag {
    last_cursor_pos: Option<[f32; 2]>,
}

impl ViewportOrbitDrag {
    #[cfg(test)]
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
    #[cfg(test)]
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
