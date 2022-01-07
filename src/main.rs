// https://github.com/emilk/egui/blob/76cdbe2cf8ebb946377c51f4545b27033d335096/eframe/examples/file_dialog.rs
// https://github.com/emilk/egui/discussions/161
// https://github.com/emilk/egui/discussions?discussions_q=grid
// https://github.com/EmbarkStudios/puffin
// https://www.reddit.com/r/rust/comments/rsa072/building_a_fully_static_linux_executable_in_2021/

//use std::fmt::format;
use eframe::{egui, epi};
//use storage_instance::*;
//use std::ptr::null;
use eframe::egui::Ui;
//use num_traits::identities::Zero;

#[path = "clouds/dropbox.rs"]
mod dropbox;
#[path = "clouds/clouds.rs"]
mod clouds;
#[path = "clouds/body_aggregator.rs"]
mod body_aggregator;
#[path = "clouds/call_messages.rs"]
mod call_messages;
mod config;
mod http_server;
mod storages;
mod init;
mod storage_instance;
mod call_states;
mod storage_models;
mod common_types;

use common_types::*;
use storage_instance::{
    StorageInstance
};

pub struct AppState {
    config: config::AppConfig,
    rth: RuntimeHolder,
    storages: Vec<StorageInstance>,
}

impl AppState {
    fn render_folder(storage: &mut StorageInstance, ui: &mut Ui) {

//        let mut scroll_area = egui::ScrollArea::vertical()
//            .max_height(5000.0)
//            .auto_shrink([false; 2]);
//                let mut hmg = storage.calls_info.write().unwrap();
            for (call_id, state) in storage.calls_info.iter() {
//                    if let Some(call_info) = val {
                match &state.data {
                    call_states::Data::ListFolder { .. } => {},
                    call_states::Data::DownloadFile {
                        data,
                        remote_path,
                        local_path: _,
                        size,
                        downloaded
                    } => {
                        match data {
                            call_states::DownloadFile::Ok => {}
                            call_states::DownloadFile::Failed(_) => {}
                            call_states::DownloadFile::Cancelled => {}
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
                                            let _ = storage.events_sender.send(call_messages::Message {
                                                call_id: *call_id,
                                                data: call_messages::Data::DownloadFile(
                                                    call_messages::DownloadFile::Cancelled
                                                )
                                            });
                                        }
                                        ui.label(remote_path);
                                    });
                                }
                            }
                            call_states::DownloadFile::RefreshToken => {}
                            call_states::DownloadFile::RefreshTokenComplete => {}
                        }
                    }
                }
//                    }
            }
            ui.add_space(3.0);


            let scroll_area = egui::ScrollArea::vertical()
                .max_height(4000.0)
                .auto_shrink([false; 2]);

//            ui.add_sized(ui.available_size(),
        //        let text_style = egui::TextStyle::Body;
    //        let row_height = ui.fonts()[text_style].row_height();
            let (_current_scroll, _max_scroll) = scroll_area.show(ui, |ui| {
    //        let (current_scroll, max_scroll) = scroll_area.show_rows(ui, row_height, num_rows, |ui, row_range| {
    //            if scroll_top {
    //                ui.scroll_to_cursor(egui::Align::TOP);
    //            }
                ui.vertical(|ui| {
                    egui::Grid::new("files.")
                        .num_columns(4)
    //                    .striped(true)
                        .min_col_width(5.0)
                        .start_row(2)
    //            .max_col_width(100.0)
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

    //                        ui.end_row();

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
                                    }
                                    else {
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
                                }
                                else {
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
                                    None => ui.label("dir")
                                };

                                match item.modified {
                                    Some(modified) => ui.label(modified.to_string()),
                                    None => ui.label("")
                                };
                                ui.end_row();
                            }
                        });
                });

    //            if scroll_bottom {
    //                ui.scroll_to_cursor(Align::BOTTOM);
    //            }
                let margin = ui.visuals().clip_rect_margin;
                let current_scroll = ui.clip_rect().top() - ui.min_rect().top() + margin;
                let max_scroll = ui.min_rect().height() - ui.clip_rect().height() + 2.0 * margin;
                (current_scroll, max_scroll)
            });
    }

    fn render_storage(ui: &mut Ui, storage: &mut StorageInstance) {
//        ui.heading(&storage.caption);
//        ui.add(egui::Slider::new(age, 0..=120).text("age"));
//        if ui.button("Click each year ".to_owned() + &age.to_string()).clicked() {
//            *age += 1;
//        }
//        ui.label(format!("Hellox '{}', age {}", name, age));

        let in_progress = {
//            let hmg = storage.calls_info.write().unwrap();
            let res: Vec<String> = storage.calls_info.iter().map(|(_, state)|
                match &state.data {
                    call_states::Data::ListFolder { data, path } => {
                        let action = format!("ListFolder({})", path);
                        match data {
                            call_states::ListFolder::Ok => format!("{}(Ok)", action),
                            call_states::ListFolder::Failed(e) => format!("{}(Failed: {})", action, e.to_string()),
                            call_states::ListFolder::Cancelled => format!("{}(Cancelled)", action),
                            call_states::ListFolder::InProgress { value, value_str } =>
                                format!("{}(InProgress: (value: {:?}, value_str: {:?}))", action, value, value_str),
                            call_states::ListFolder::RefreshToken => format!("{}(RefreshToken)", action),
                            call_states::ListFolder::RefreshTokenComplete => format!("{}(RefreshTokenComplete)", action),
                        }
                    },
                    //format!("ListFolder({})", path),
                    call_states::Data::DownloadFile { data, remote_path, local_path: _, size: _, downloaded: _ } => {
                        let action = format!("Download({})", remote_path);
                        match data {
                            call_states::DownloadFile::Ok => format!("{}(Ok)", action),
                            call_states::DownloadFile::Failed(e) => format!("{}(Failed: {})", action, e.to_string()),
                            call_states::DownloadFile::Cancelled => format!("{}(Cancelled)", action),
                            call_states::DownloadFile::Started => format!("{}(Started)", action),
                            call_states::DownloadFile::SizeInfo { size } => format!("{}(SizeInfo: {:?})", action, size),
                            call_states::DownloadFile::InProgress { progress } => format!("{}(InProgress: {:?})", action, progress),
                            call_states::DownloadFile::RefreshToken => format!("{}(RefreshToken)", action),
                            call_states::DownloadFile::RefreshTokenComplete => format!("{}(RefreshTokenComplete)", action),
                        }
                    }
                }
            ).collect();
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

//            ui.horizontal(|ui| {
//            });
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            match &storage.visual_state.folder {
                Some(_folder) => {
//                storage.visual_state.v_path = "/".to_owned() + &folder.path;
                    ui.horizontal(|ui| {
                        ui.label("Path:");
                        ui.text_edit_singleline(&mut storage.visual_state.v_path);
                    });
                    Self::render_folder(storage, ui);
                }
                _ => {
//                panic!();
                }
            };
        });
    }

    fn render(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
        let Self { config: _, rth: _, storages } = self;
        egui::CentralPanel::default().show(ctx, |ui| {
//            let storage = &storages[0];

            let mut idx = 0;
            for storage in storages {
                if storage.do_auth {
                    storage.do_auth = false;
                    storage.start_auth();
                }
                storage.prepare_visual_state();
                if idx == 0 {
                    Self::render_storage(ui, storage);
//                    ui.add(egui::Slider::new(age, 0..=120).text("age"));
                }
                idx = idx + 1;
            }

//            ui.horizontal(|ui| {
//                ui.label("Info: ".to_owned() + info);
//            });
        });

//        frame.set_window_size(ctx.used_size());
    }

    fn cleanup(&mut self) {
        let rth = self.rth.clone();
        let mut g = rth.write().unwrap();
        let rto = g.take();
        let rt = rto.unwrap();

        rt.block_on(async {
            for storage in &self.storages {
                let auth_info = {
                    (*storage.auth_info_holder.read().await).clone()
                };
                if let Some(storage_config) = self.config.storage_by_id_mut(storage.id.clone()) {
                    storage_config.tokens.token = auth_info.token;
                    storage_config.tokens.refresh_token = auth_info.refresh_token;
                    if let Some(folder) = &storage.visual_state.folder {
                        storage_config.current_path = folder.path.clone()
                    }
                }
            }
        });
        log::info!("rt.shutdown");
        rt.shutdown_timeout(std::time::Duration::from_millis(1000));
        let _ = config::save_config(&self.config);
    }
}

impl epi::App for AppState {
    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame) {
        self.render(ctx, frame);
        ctx.request_repaint();
    }

    fn on_exit(&mut self) {
        self.cleanup();
    }

    fn name(&self) -> &str {
        "Clouds viewer"
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app_state = init::init()?;

    for storage in app_state.storages.iter_mut() {
        if let Some(storage_config) = app_state.config.storage_by_id(storage.id.clone()) {
            storage.nav_to(storage_config.current_path.clone());
        } else {
            storage.nav_to("".to_string());
        }
    }

    let options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(app_state), options);
}
