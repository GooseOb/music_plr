use super::BackendResult;

impl super::Backend {
    fn safe_filename(&self, artist: &str, title: &str) -> String {
        let filename = format!("{} - {}", artist, title);
        filename
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub fn handle_download_track(&mut self, index: usize) {
        let track = match self.get_track_at(index) {
            Some(t) => t,
            None => return,
        };
        let download_dir = self.config.download_dir.clone();
        let output_path = format!(
            "{}/{}.%(ext)s",
            download_dir,
            self.safe_filename(&track.artist, &track.title)
        );
        self.downloading_index = Some(index);

        let track_url = track.url.clone();
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || {
            match crate::youtube::download_audio(&track_url, &output_path) {
                Ok(path) => {
                    let _ = result_tx.send(BackendResult::DownloadComplete(index, track_url, path));
                }
                Err(e) => {
                    let _ = result_tx.send(BackendResult::DownloadError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_remove_download(&mut self, index: usize) {
        let track = match self.get_track_at(index) {
            Some(t) => t,
            None => return,
        };
        if let Some(path) = self.download_registry.remove(&track.url) {
            let _ = std::fs::remove_file(&path);
            self.notify("Download removed".into());
        }
        self.sync_search_model();
        self.sync_radio_model();
        self.sync_playlist_content();
    }

    pub fn handle_download_current(&mut self) {
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        let download_dir = self.config.download_dir.clone();
        let output_path = format!(
            "{}/{}.%(ext)s",
            download_dir,
            self.safe_filename(&track.artist, &track.title)
        );

        let track_url = track.url.clone();
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || {
            match crate::youtube::download_audio(&track_url, &output_path) {
                Ok(path) => {
                    let _ = result_tx.send(BackendResult::DownloadComplete(0, track_url, path));
                }
                Err(e) => {
                    let _ = result_tx.send(BackendResult::DownloadError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_download_or_delete_at(&mut self, index: usize) {
        let track = match self.get_track_at(index) {
            Some(t) => t,
            None => return,
        };
        if self.download_registry.contains(&track.url) {
            self.handle_remove_download(index);
        } else {
            self.handle_download_track(index);
        }
    }
}
