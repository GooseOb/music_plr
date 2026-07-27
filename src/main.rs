mod audio;
mod backend;
mod config;
mod downloads;
mod mpris;
mod playlists;
mod types;
mod youtube;

use backend::Backend;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

fn main() {
    let config = config::load_config();
    let (backend, mpris_cmd_tx) = Backend::new(config);
    let backend = Rc::new(RefCell::new(backend));

    let window = backend::AppWindow::new().unwrap();
    window.window().set_maximized(true);
    {
        let mut b = backend.borrow_mut();
        b.ui = window.as_weak();
        let (mpris_tx, mpris_rx) = mpsc::channel();
        mpris::start(mpris_cmd_tx, mpris_rx);
        b.mpris_update_tx = Some(mpris_tx);
        b.update_ui();
    }

    setup_callbacks(&window, &backend);
    setup_timer(&backend);

    if !backend.borrow().config.last_search_query.is_empty() {
        let mut b = backend.borrow_mut();
        let query = b.config.last_search_query.clone();
        b.search_query = query.clone();
        if let Some(w) = b.ui.upgrade() {
            w.set_search_input_text(query.into());
        }
        b.handle_search_execute();
    }

    window.run().unwrap();
}

fn setup_timer(backend: &Rc<RefCell<Backend>>) {
    let backend_weak = Rc::downgrade(backend);
    let timer = slint::Timer::default();
    if let Some(b) = backend_weak.upgrade() {
        b.borrow_mut()._timer = Some(timer);
        if let Some(t) = &b.borrow()._timer {
            t.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(250),
                move || {
                    if let Some(b) = backend_weak.upgrade() {
                        b.borrow_mut().tick();
                    }
                },
            );
        }
    }
}

fn setup_callbacks(window: &backend::AppWindow, backend: &Rc<RefCell<Backend>>) {
    let b = Rc::downgrade(backend);
    window.on_navigate_to(move |_view| {
        if let Some(b) = b.upgrade() {
            let mut b = b.borrow_mut();
            b.selected_playlist = None;
            b.selected_playlist_name.clear();
            b.handle_navigate_to(backend::View::Search);
        }
    });

    {
        let b = Rc::downgrade(backend);
        window.on_navigate_back(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_navigate_back();
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_navigate_forward(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_navigate_forward();
            }
        });
    }

    let b = Rc::downgrade(backend);
    window.on_search_input_changed(move |text| {
        if let Some(b) = b.upgrade() {
            let mut b = b.borrow_mut();
            b.search_query = text.to_string();
            if let Some(w) = b.ui.upgrade() {
                b.update_search_history(&w);
                w.set_show_search_history(true);
            }
        }
    });

    let b = Rc::downgrade(backend);
    window.on_search_execute(move || {
        if let Some(b) = b.upgrade() {
            let mut b = b.borrow_mut();
            if let Some(w) = b.ui.upgrade() {
                w.set_show_search_history(false);
            }
            b.handle_search_execute();
        }
    });

    {
        let b = Rc::downgrade(backend);
        window.on_search_load_more(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_search_load_more();
            }
        });
    }

    let b = Rc::downgrade(backend);
    window.on_search_history_selected(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_search_history_select(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_delete_search_history(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_delete_search_history(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_play_track(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_play_track(index as usize);
        }
    });

    {
        let b = Rc::downgrade(backend);
        window.on_toggle_play_pause(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_toggle_play_pause();
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_next_track(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_next_track();
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_previous_track(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_previous_track();
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_set_volume(move |vol| {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_set_volume(vol);
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_seek(move |frac| {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_seek(frac);
            }
        });
    }

    let b = Rc::downgrade(backend);
    window.on_start_song_radio(move |name| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_start_song_radio(name.to_string());
        }
    });

    let b = Rc::downgrade(backend);
    window.on_start_artist_radio(move |name| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_start_artist_radio(name.to_string());
        }
    });

    let b = Rc::downgrade(backend);
    window.on_download_track(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_download_track(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_remove_download(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_remove_download(index as usize);
        }
    });

    {
        let b = Rc::downgrade(backend);
        window.on_download_current(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_download_current();
            }
        });
    }

    let b = Rc::downgrade(backend);
    window.on_add_local_music(move || {
        if let Some(b) = b.upgrade() {
            let files = rfd::FileDialog::new()
                .add_filter("Audio", &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"])
                .pick_files();
            if let Some(files) = files {
                let paths: Vec<String> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();
                b.borrow_mut().handle_add_local_music(paths);
            }
        }
    });

    {
        let b = Rc::downgrade(backend);
        window.on_create_playlist(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_create_playlist();
            }
        });
    }

    let b = Rc::downgrade(backend);
    window.on_delete_playlist(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_delete_playlist(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_select_playlist(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_select_playlist(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_toggle_picker(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_toggle_picker(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_add_to_playlist(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_add_to_playlist(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_drag_add_to_playlist(move |track_idx, playlist_idx| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_drag_add_to_playlist(track_idx as usize, playlist_idx as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_remove_from_playlist(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_remove_from_playlist(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_close_picker(move || {
        if let Some(b) = b.upgrade() {
            let mut b = b.borrow_mut();
            b.show_playlist_picker = None;
            if let Some(w) = b.ui.upgrade() {
                w.set_show_picker(false);
            }
        }
    });

    let b = Rc::downgrade(backend);
    window.on_playlist_name_changed(move |text| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().playlist_create_name = text.to_string();
        }
    });

    let b = Rc::downgrade(backend);
    window.on_toggle_select(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_toggle_select(index as usize);
        }
    });

    {
        let b = Rc::downgrade(backend);
        window.on_copy_selected(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_copy_selected();
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_delete_selected(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_delete_selected();
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_paste_clipboard(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_paste_clipboard();
            }
        });
    }
    {
        let b = Rc::downgrade(backend);
        window.on_clear_selection(move || {
            if let Some(b) = b.upgrade() {
                b.borrow_mut().handle_clear_selection();
            }
        });
    }

    let b = Rc::downgrade(backend);
    window.on_start_radio(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_radio_at(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_start_artist(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_artist_at(index as usize);
        }
    });

    let b = Rc::downgrade(backend);
    window.on_download_or_delete(move |index| {
        if let Some(b) = b.upgrade() {
            b.borrow_mut().handle_download_or_delete_at(index as usize);
        }
    });
}
