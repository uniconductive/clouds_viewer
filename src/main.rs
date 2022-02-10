use eframe::{
    egui::{self, Ui},
    epi,
};

#[path = "clouds/call_messages.rs"]
mod call_messages;
mod call_states;
#[path = "clouds/clouds.rs"]
mod clouds;
mod common_types;
mod config;
#[path = "clouds/dropbox.rs"]
mod dropbox;
mod error;
mod http_server;
mod init;
mod storage_instance;
mod storage_models;
mod storages;

use common_types::*;
use storage_instance::StorageInstance;

pub struct AppState {
    config: config::AppConfig,
    rth: RuntimeHolder,
    storages: Vec<StorageInstance>,
}

impl AppState {
    fn render_downloads(storage: &mut StorageInstance, ui: &mut Ui) {
        for (call_id, state) in storage.call_states.iter() {
            match &state.data {
                call_states::Data::ListFolder { .. } => {}
                call_states::Data::Auth { .. } => {}
                call_states::Data::DownloadFile {
                    data,
                    remote_path,
                    local_path: _,
                    size,
                    downloaded,
                } => match data {
                    call_states::DownloadFile::Ok => {}
                    call_states::DownloadFile::Failed(_) => {}
                    //                    call_states::DownloadFile::Cancelled => {}
                    call_states::DownloadFile::Started => {}
                    call_states::DownloadFile::SizeInfo { .. } => {}
                    call_states::DownloadFile::InProgress { .. } => {
                        if let Some(size) = size {
                            let progress = *downloaded as f32 / *size as f32;
                            let progress_bar = egui::ProgressBar::new(progress)
                                .desired_width(100.0)
                                .show_percentage();
                            ui.horizontal(|ui| {
                                ui.add(progress_bar);
                                if ui.button("x").clicked() {
                                    storage.cancel_download_file(*call_id);
                                }
                                ui.label(remote_path);
                            });
                        }
                    }
                    call_states::DownloadFile::RefreshToken => {}
                    call_states::DownloadFile::RefreshTokenComplete => {}
                },
            }
        }
    }

    fn render_folder(storage: &mut StorageInstance, ui: &mut Ui) {
        let scroll_area = egui::ScrollArea::vertical()
            .max_height(4000.0)
            .auto_shrink([false; 2]);

        let (_current_scroll, _max_scroll) = scroll_area.show(ui, |ui| {
            ui.vertical(|ui| {
                egui::Grid::new("files.")
                    .num_columns(4)
                    .min_col_width(5.0)
                    .start_row(2)
                    //                    .max_col_width(100.0)
                    .show(ui, |ui| {
                        let level_up_needed = {
                            let folder = storage.visual_state.folder.as_ref().unwrap();
                            !folder.path.is_empty()
                        };

                        if ui.selectable_label(false, "Name").clicked() {
                            let folder = storage.visual_state.folder.as_mut().unwrap();
                            folder.items.sort_by(|a, b| a.name.cmp(&b.name));
                        }
                        ui.label("");
                        if ui.selectable_label(false, "Size").clicked() {
                            let folder = storage.visual_state.folder.as_mut().unwrap();
                            folder.items.sort_by(|a, b| a.size.cmp(&b.size));
                        }
                        if ui.selectable_label(false, "Time").clicked() {
                            let folder = storage.visual_state.folder.as_mut().unwrap();
                            folder.items.sort_by(|a, b| a.modified.cmp(&b.modified));
                        }
                        ui.end_row();

                        if level_up_needed {
                            if ui.selectable_label(false, "..").clicked() {
                                storage.nav_back();
                            }
                            if ui.selectable_label(false, "up ").clicked() {
                                storage.nav_back();
                            }
                            ui.label("");
                            ui.end_row();
                        }
                        let folder = storage.visual_state.folder.as_ref().unwrap();
                        for item in &folder.items {
                            if ui.selectable_label(false, &item.name).clicked() {
                                if item.is_folder {
                                    storage.nav_forward(item.name.clone());
                                } else {
                                    let remote_file_path = if folder.path.is_empty() {
                                        item.name.clone()
                                    } else {
                                        folder.path.clone() + "/" + &item.name
                                    };
                                    let local_file_path = storage.save_to_path.clone() + &item.name;
                                    storage.download_file(remote_file_path, local_file_path);
                                }
                            }
                            if item.is_folder {
                                ui.label("");
                            } else {
                                if ui.button("download").clicked() {
                                    let remote_file_path = if folder.path.is_empty() {
                                        item.name.clone()
                                    } else {
                                        folder.path.clone() + "/" + &item.name
                                    };
                                    let local_file_path = storage.save_to_path.clone() + &item.name;
                                    storage.download_file(remote_file_path, local_file_path);
                                }
                            }
                            match item.size {
                                Some(size) => ui.label(size.to_string()),
                                None => ui.label("dir"),
                            };

                            match item.modified {
                                Some(modified) => ui.label(modified.to_string()),
                                None => ui.label(""),
                            };
                            ui.end_row();
                        }
                    });
            });

            let margin = ui.visuals().clip_rect_margin;
            let current_scroll = ui.clip_rect().top() - ui.min_rect().top() + margin;
            let max_scroll = ui.min_rect().height() - ui.clip_rect().height() + 2.0 * margin;
            (current_scroll, max_scroll)
        });
    }

    fn render_state(ui: &mut Ui, storage: &mut StorageInstance) {
        let in_progress = {
            let res: Vec<String> = storage
                .call_states
                .iter()
                .map(|(_, state)| match &state.data {
                    call_states::Data::ListFolder { data, path } => {
                        let action = format!("ListFolder({})", path);
                        match data {
                            call_states::ListFolder::Ok => format!("{}(Ok)", action),
                            call_states::ListFolder::Failed(e) => {
                                format!("{}(Failed: {})", action, e)
                            }
                            call_states::ListFolder::InProgress { value, value_str } => format!(
                                "{}(InProgress: (value: {:?}, value_str: {:?}))",
                                action, value, value_str
                            ),
                            call_states::ListFolder::RefreshToken => {
                                format!("{}(RefreshToken)", action)
                            }
                            call_states::ListFolder::RefreshTokenComplete => {
                                format!("{}(RefreshTokenComplete)", action)
                            }
                        }
                    }
                    call_states::Data::DownloadFile {
                        data,
                        remote_path,
                        local_path: _,
                        size: _,
                        downloaded: _,
                    } => {
                        let action = format!("Download({})", remote_path);
                        match data {
                            call_states::DownloadFile::Ok => format!("{}(Ok)", action),
                            call_states::DownloadFile::Failed(e) => {
                                format!("{}(Failed: {})", action, e)
                            }
                            call_states::DownloadFile::Started => format!("{}(Started)", action),
                            call_states::DownloadFile::SizeInfo { size } => {
                                format!("{}(SizeInfo: {:?})", action, size)
                            }
                            call_states::DownloadFile::InProgress { progress } => {
                                format!("{}(InProgress: {:?})", action, progress)
                            }
                            call_states::DownloadFile::RefreshToken => {
                                format!("{}(RefreshToken)", action)
                            }
                            call_states::DownloadFile::RefreshTokenComplete => {
                                format!("{}(RefreshTokenComplete)", action)
                            }
                        }
                    }
                    call_states::Data::Auth { data } => {
                        let action = "Auth".to_string();
                        match data {
                            call_states::Auth::Ok => format!("{}(Ok)", action),
                            call_states::Auth::Failed(e) => format!("{}(Failed: {})", action, e),
                            call_states::Auth::InProgress(progress) => {
                                format!("{}(InProgress({:?}))", action, progress)
                            }
                        }
                    }
                })
                .collect();
            res
        };

        if !in_progress.is_empty() {
            egui::TopBottomPanel::bottom("bottom_panel")
                .resizable(false)
                .min_height(0.0)
                .show_inside(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("progress");
                        for item in &in_progress {
                            ui.label(item);
                        }
                    });
                });
        }
    }

    fn render_menu(ui: &mut Ui, storage: &mut StorageInstance) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("cloud", |ui| {
                if ui.button("log in...").clicked() {
                    storage.start_auth();
                    ui.close_menu();
                }
            });
        });
    }

    fn render_storage(_ctx: &egui::CtxRef, ui: &mut Ui, storage: &mut StorageInstance) {
        Self::render_menu(ui, storage);
        Self::render_state(ui, storage);

        if let Some(auth_state) = &storage.visual_state.auth_process_state {
            ui.horizontal(|ui| {
                if ui.button("Cancel auth").clicked() {
                    storage.cancel_auth(auth_state.call_id);
                }
            });
        } else {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                match &storage.visual_state.folder {
                    Some(_folder) => {
                        ui.horizontal(|ui| {
                            ui.label("Path: ");
                            ui.text_edit_singleline(&mut storage.visual_state.v_path);
                        });
                        Self::render_downloads(storage, ui);
                        ui.add_space(3.0);
                        Self::render_folder(storage, ui);
                    }
                    _ => {}
                };
            });
        }
    }

    fn render(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
        let Self { storages, .. } = self;
        egui::CentralPanel::default().show(ctx, |ui| {
            for (idx, storage) in storages.iter_mut().enumerate() {
                storage.prepare_visual_state();
                if idx == 0 {
                    Self::render_storage(ctx, ui, storage);
                }
            }
        });

        //        frame.set_window_size(ctx.used_size());
    }

    fn cleanup(&mut self) {
        self.rth
            .block_on(async {
                for storage in &self.storages {
                    let auth_info = { (*storage.auth_info_holder.read().await).clone() };
                    if let Some(storage_config) = self.config.storage_by_id_mut(storage.id.clone())
                    {
                        storage_config.tokens.token = auth_info.token;
                        storage_config.tokens.refresh_token = auth_info.refresh_token;
                        if let Some(folder) = &storage.visual_state.folder {
                            storage_config.current_path = folder.path.clone()
                        }
                    }
                }
            })
            .unwrap();

        let _ = config::save_config(&self.config);

        self.rth.print_hyper_futures_states();

        log::debug!("rt.shutdown.started");
        let _ = self
            .rth
            .shutdown_timeout(std::time::Duration::from_secs(10));
        log::debug!("rt.shutdown.finished");
    }
}

impl epi::App for AppState {
    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame) {
        self.render(ctx, frame);
        ctx.request_repaint();
    }

    fn setup(
        &mut self,
        _ctx: &egui::CtxRef,
        _frame: &epi::Frame,
        _storage: Option<&dyn epi::Storage>,
    ) {
        let mut fonts = egui::FontDefinitions::default();
        let font_bytes = include_bytes!("../resources/fonts/mplus-1p-regular.ttf");
        fonts.font_data.insert(
            "mplus1p".to_owned(),
            egui::FontData::from_static(font_bytes),
        );

        fonts
            .fonts_for_family
            .get_mut(&egui::FontFamily::Monospace)
            .unwrap()
            .insert(0, "mplus1p".to_owned());
        fonts
            .fonts_for_family
            .get_mut(&egui::FontFamily::Proportional)
            .unwrap()
            .insert(0, "mplus1p".to_owned());

        for style in egui::TextStyle::all() {
            fonts
                .family_and_size
                .insert(style, (egui::FontFamily::Proportional, 18.0));
        }

        _ctx.set_fonts(fonts);
    }

    fn on_exit(&mut self) {
        self.cleanup();
    }

    fn name(&self) -> &str {
        "Clouds viewer "
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app_state = init::init()?;

    for storage in app_state.storages.iter_mut() {
        if storage.visual_state.auth_needed {
            storage.visual_state.auth_needed = false;
            storage.start_auth();
        }

        if let Some(storage_config) = app_state.config.storage_by_id(storage.id.clone()) {
            storage.nav_to(storage_config.current_path.clone());
        } else {
            storage.nav_to("".to_string());
        }
    }

    let options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(app_state), options);
}
