use crate::config;

impl super::Backend {
    pub fn handle_play_track(&mut self, index: usize) {
        let tracks = self.get_current_tracks();
        if tracks.is_empty() {
            return;
        }
        let track = match tracks.get(index) {
            Some(t) => t.clone(),
            None => return,
        };
        self.queue = super::PlayQueue {
            tracks: tracks.clone(),
            current_index: index,
        };

        self.play_track_internal(&track);
    }

    pub fn handle_toggle_play_pause(&mut self) {
        if self.is_playing {
            self.audio.pause();
            self.is_playing = false;
        } else if self.queue.current().is_some() {
            self.audio.resume();
            self.is_playing = true;
        }
    }

    pub fn handle_next_track(&mut self) {
        if self.queue.next().is_none() {
            return;
        }
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        self.play_track_internal(&track);
    }

    pub fn handle_previous_track(&mut self) {
        if self.queue.previous().is_none() {
            return;
        }
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        self.play_track_internal(&track);
    }

    pub fn handle_set_volume(&mut self, vol: f32) {
        self.volume = vol;
        self.config.volume = vol;
        self.audio.set_volume(vol);
        config::save_config(&self.config);
    }

    pub fn handle_seek(&mut self, frac: f32) {
        let pos = std::time::Duration::from_secs_f32(frac * self.duration);
        self.progress = frac;
        self.audio.seek(pos);
    }
}
