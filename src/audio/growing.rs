use std::{
    io::{self, SeekFrom},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

/// A symphonia `MediaSource` over a cache file that yt-dlp is still writing.
///
/// Reports itself as **non-seekable** while the download is running so
/// symphonia's format readers demux *sequentially* during initialization. On
/// a partial file a seek would hit missing bytes and trip rodio's
/// `unreachable!("Seek errors should not occur during initialization")`.
/// Reads at EOF *block* (with a short sleep) while the writer is alive, so the
/// decoder sees a file that grows until the download finishes — at which
/// point a real `EOF` is reported and the track ends normally. Once the
/// writer is done the file is whole, so the source becomes seekable and
/// in-place seeks (e.g. the progress bar) work without replaying.
pub(super) struct GrowingMediaSource {
    pub(super) file: std::fs::File,
    /// `Some(flag)` while the copy thread is still writing: reads block at
    /// EOF until the flag flips to `false`. `None` means the download is
    /// already complete, so a 0-byte read is a genuine EOF.
    pub(super) writer_alive: Option<Arc<AtomicBool>>,
}

impl GrowingMediaSource {
    fn download_done(&self) -> bool {
        self.writer_alive
            .as_ref()
            .is_none_or(|w| !w.load(Ordering::Acquire))
    }
}

impl io::Read for GrowingMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = std::time::Instant::now();
        loop {
            match self.file.read(buf) {
                Ok(0) => {
                    let still_writing = self
                        .writer_alive
                        .as_ref()
                        .is_some_and(|w| w.load(Ordering::Acquire));
                    if !still_writing {
                        return Ok(0);
                    }
                    if start.elapsed() > Duration::from_mins(10) {
                        return Ok(0);
                    }
                    // Writer still has bytes coming; wait briefly and retry
                    // rather than signalling premature EOF.
                    thread::sleep(Duration::from_millis(15));
                }
                result => return result,
            }
        }
    }
}

impl io::Seek for GrowingMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        // Seeking is only valid once the download is complete; during live
        // streaming the source is intentionally non-seekable.
        if self.download_done() {
            self.file.seek(pos)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "streaming source is not seekable",
            ))
        }
    }
}

impl symphonia::core::io::MediaSource for GrowingMediaSource {
    fn is_seekable(&self) -> bool {
        // Live (partial) files are non-seekable so symphonia demuxes
        // sequentially; the probe then never seeks (and never hits the
        // `byte_len() == None` seek-error panic inside rodio's `Decoder`).
        // Once the download is done the file is whole, so the still-playing
        // source becomes seekable without a replay.
        self.download_done()
    }

    fn byte_len(&self) -> Option<u64> {
        if self.download_done() {
            self.file.metadata().ok().map(|m| m.len())
        } else {
            None
        }
    }
}
