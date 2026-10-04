use eframe::egui;

const DEFAULT_SPEED_LAYERS_PER_SECOND: f32 = 15.0;
const MAX_SPEED_LAYERS_PER_SECOND: f32 = 120.0;

pub struct LayerPlayer {
    playing: bool,
    speed: f32,
    progress: f32,
}

impl LayerPlayer {
    pub fn new() -> Self {
        Self {
            playing: false,
            speed: DEFAULT_SPEED_LAYERS_PER_SECOND,
            progress: 0.0,
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    /// Starts from the bottom layer when already at the top, otherwise resumes.
    pub fn toggle(&mut self, first_layer: u32, last_layer: &mut u32, top_layer: u32) {
        if self.playing {
            self.playing = false;
            return;
        }
        self.progress = if *last_layer >= top_layer {
            first_layer as f32
        } else {
            *last_layer as f32
        };
        *last_layer = self.progress as u32;
        self.playing = true;
    }

    /// Advances the top layer. Returns true while playback continues.
    pub fn advance(
        &mut self,
        delta_seconds: f32,
        first_layer: u32,
        last_layer: &mut u32,
        top_layer: u32,
    ) -> bool {
        if !self.playing {
            return false;
        }
        self.progress += delta_seconds * self.speed;
        if self.progress >= top_layer as f32 {
            *last_layer = top_layer;
            self.playing = false;
            return false;
        }
        *last_layer = (self.progress as u32).max(first_layer);
        true
    }

    pub fn show_controls(
        &mut self,
        ui: &mut egui::Ui,
        first_layer: u32,
        last_layer: &mut u32,
        top_layer: u32,
    ) {
        ui.horizontal(|ui| {
            let label = if self.playing { "Pause" } else { "Play" };
            if ui.button(label).clicked() {
                self.toggle(first_layer, last_layer, top_layer);
            }
            ui.spacing_mut().slider_width = ui.available_width();
            ui.add(
                egui::Slider::new(&mut self.speed, 1.0..=MAX_SPEED_LAYERS_PER_SECOND)
                    .show_value(false)
                    .logarithmic(true),
            )
            .on_hover_text(format!("{:.0} layers per second", self.speed));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_restart_from_bottom_when_toggled_at_top() {
        let mut player = LayerPlayer::new();
        let mut last = 10;
        player.toggle(2, &mut last, 10);
        assert_eq!(last, 2);
        assert!(player.playing);
    }

    #[test]
    fn should_advance_and_stop_at_top() {
        let mut player = LayerPlayer::new();
        let mut last = 0;
        player.toggle(0, &mut last, 30);
        assert!(player.advance(1.0, 0, &mut last, 30));
        assert_eq!(last, 15);
        assert!(!player.advance(2.0, 0, &mut last, 30));
        assert_eq!(last, 30);
    }

    #[test]
    fn should_resume_from_current_layer() {
        let mut player = LayerPlayer::new();
        let mut last = 7;
        player.toggle(0, &mut last, 30);
        assert_eq!(last, 7);
    }
}
