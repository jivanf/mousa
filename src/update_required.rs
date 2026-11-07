//! Compatibility recovery, isolated from library, account and playback startup.
use crate::{
    theme::{border, color, font, landing, shadow, space, text},
    ui,
};
use gpui::{prelude::*, *};
use std::{path::PathBuf, time::Duration};

pub struct UpdateRequired {
    busy: bool,
    error: Option<String>,
    preview: bool,
    complete: bool,
}
impl Default for UpdateRequired {
    fn default() -> Self {
        Self::new()
    }
}
impl UpdateRequired {
    pub fn new() -> Self {
        Self {
            busy: false,
            error: None,
            preview: false,
            complete: false,
        }
    }
    pub fn preview(state: &str) -> Self {
        Self {
            busy: state == "updating",
            error: (state == "error").then(|| {
                "Could not download the update. Check your connection and try again.".into()
            }),
            preview: true,
            complete: state == "success",
        }
    }
    fn update(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.complete = false;
        let preview = self.preview;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if preview {
                        std::thread::sleep(Duration::from_secs(1));
                        return Ok(());
                    }
                    request_update()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) if preview => this.complete = true,
                    Ok(()) => cx.quit(),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
fn request_update() -> Result<(), String> {
    let directory = std::env::var_os("MOUSA_UPDATE_CHANNEL").map(PathBuf::from)
        .ok_or("Automatic update is unavailable in this installation. Install the latest Mousa release, then reopen the app.".to_owned())?;
    request_update_at(&directory)
}
fn request_update_at(directory: &std::path::Path) -> Result<(), String> {
    let response = directory.join("response");
    if response.exists() {
        std::fs::remove_file(&response).map_err(|e| e.to_string())?;
    }
    std::fs::write(directory.join("request"), b"update").map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    loop {
        if let Ok(value) = std::fs::read_to_string(&response) {
            return if value == "ready" { Ok(()) } else { Err(value) };
        }
        if started.elapsed() > Duration::from_secs(600) {
            return Err("The update timed out. Please try again.".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
#[cfg(test)]
mod tests {
    use super::request_update_at;
    use std::time::Duration;
    #[test]
    fn update_channel_reports_errors_and_allows_retry() {
        let directory = tempfile::tempdir().unwrap();
        for response in ["Download failed", "ready"] {
            let path = directory.path().to_owned();
            let worker = std::thread::spawn(move || {
                while !path.join("request").exists() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                std::fs::remove_file(path.join("request")).unwrap();
                let temp = path.join("response.tmp");
                std::fs::write(&temp, response).unwrap();
                std::fs::rename(temp, path.join("response")).unwrap();
            });
            let result = request_update_at(directory.path());
            worker.join().unwrap();
            assert_eq!(
                result,
                if response == "ready" {
                    Ok(())
                } else {
                    Err(response.into())
                }
            );
        }
    }
}
impl Render for UpdateRequired {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content_width = landing::width() - landing::padding() * 2.;
        let action = if self.busy || self.complete {
            ui::disabled_button(if self.complete {
                "RESTARTING…"
            } else {
                "UPDATING…"
            })
            .w_full()
            .h(crate::theme::update_required::button_height())
            .mr(crate::theme::press::md())
            .mb(crate::theme::press::md())
            .justify_center()
            .into_any_element()
        } else {
            ui::button("required-update", "UPDATE", color::lime())
                .w_full()
                .h(crate::theme::update_required::button_height())
                .shadow(shadow::md())
                .mr(crate::theme::press::md())
                .mb(crate::theme::press::md())
                .on_click(cx.listener(|this, _, _, cx| this.update(cx)))
                .into_any_element()
        };
        let error = self.error.clone().map(|message| {
            ui::error_alert(
                message,
                ui::button("dismiss-required-update-error", "DISMISS", color::surface()).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.error = None;
                        cx.notify();
                    }),
                ),
            )
        });
        div().size_full().bg(color::canvas()).text_color(color::ink())
            .font_family(font::SANS).font_weight(FontWeight::MEDIUM)
            .flex().items_center().justify_center().p(space::xxl())
            .child(div().w(landing::width()).p(landing::padding())
                .border(border::strong()).border_color(color::ink()).bg(color::surface())
                .shadow(shadow::lg()).flex().flex_col().gap(space::xxl())
                .child(div().size(landing::logo()).flex_shrink_0().border(border::strong()).border_color(color::ink())
                    .bg(color::lime()).shadow(shadow::md())
                    .child(img(ImageSource::Resource(Resource::Embedded("mousa.svg".into()))).size_full()))
                .child(div().flex().flex_col().gap(space::sm())
                    .child(div().text_size(text::page_title()).line_height(relative(1.2)).child("Mousa needs an update"))
                    .child(div().text_size(crate::theme::update_required::body()).line_height(relative(1.5)).text_color(color::muted())
                        .child("A recent change means this version can no longer work. Restart Mousa to install the latest version and keep listening.")))
                .child(div().w(content_width).flex().flex_col().gap(space::lg()).children(error).child(action)))
    }
}
