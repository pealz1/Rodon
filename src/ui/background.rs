use crate::themes::Background;
use eframe::egui;
use image::{AnimationDecoder, DynamicImage, RgbaImage};
use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Media {
    key: Option<(String, u8)>,
    stop: Option<Arc<AtomicBool>>,
    frames: Option<mpsc::Receiver<egui::ColorImage>>,
    texture: Option<egui::TextureHandle>,
}
impl Drop for Media {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            stop.store(true, Ordering::Release);
        }
    }
}
impl Media {
    pub fn paint(&mut self, ui: &egui::Ui, settings: Option<&Background>) {
        let selected = settings.filter(|b| b.enabled && !b.path.is_empty());
        if self.key.as_ref().map(|(p, b)| (p.as_str(), *b)) != selected.map(|b| (b.path.as_str(), b.blur)) {
            *self = Self::default();
            if let Some(bg) = selected {
                let key = (bg.path.clone(), bg.blur);
                self.key = Some(key.clone());
                let stop = Arc::new(AtomicBool::new(false));
                self.stop = Some(stop.clone());
                let (send, rx) = mpsc::sync_channel(1);
                self.frames = Some(rx);
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let publish = |img: RgbaImage, delay: Duration| {
                        if stop.load(Ordering::Acquire) {
                            return false;
                        }
                        let start = Instant::now();
                        let img = DynamicImage::ImageRgba8(img);
                        let img = img.thumbnail(img.width().min(1280), img.height().min(800));
                        let img = if key.1 > 0 { img.blur(f32::from(key.1) * 0.12) } else { img };
                        let frame = egui::ColorImage::from_rgba_unmultiplied(
                            [img.width() as usize, img.height() as usize],
                            img.to_rgba8().as_raw(),
                        );
                        if send.send(frame).is_err() {
                            return false;
                        }
                        ctx.request_repaint();
                        let delay = delay.min(Duration::from_secs(60));
                        while start.elapsed() < delay && !stop.load(Ordering::Acquire) {
                            std::thread::sleep(delay.saturating_sub(start.elapsed()).min(Duration::from_millis(30)));
                        }
                        !stop.load(Ordering::Acquire)
                    };
                    if let Err(e) = decode(Path::new(&key.0), publish) {
                        crate::log_warn!("Background: {e}");
                    }
                });
            }
        }
        if let Some(rx) = &self.frames
            && let Ok(frame) = rx.try_recv()
        {
            if let Some(t) = &mut self.texture {
                t.set(frame, egui::TextureOptions::LINEAR);
            } else {
                self.texture = Some(ui.ctx().load_texture("theme background", frame, egui::TextureOptions::LINEAR));
            }
        }
        if let (Some(t), Some(bg)) = (&self.texture, selected) {
            let rect = ui.max_rect();
            let size = t.size_vec2();
            let scale = (rect.width() / size.x).max(rect.height() / size.y);
            let visible = rect.size() / (size * scale);
            let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), visible);
            ui.painter().image(t.id(), rect, uv, egui::Color32::WHITE);
            let tint = super::theme::parse_hex(&bg.tint).unwrap_or(egui::Color32::BLACK);
            ui.painter().rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), (u16::from(bg.dim) * 255 / 100) as u8),
            );
        }
    }
}
fn decode(path: &Path, send: impl Fn(RgbaImage, Duration) -> bool) -> Result<(), Box<dyn std::error::Error>> {
    let extension = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
    if extension == "gif" {
        loop {
            let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(File::open(path)?))?;
            let mut seen = false;
            for frame in decoder.into_frames() {
                let frame = frame?;
                let (n, d) = frame.delay().numer_denom_ms();
                seen = true;
                if !send(frame.into_buffer(), Duration::from_millis(u64::from((n / d.max(1)).max(10)))) {
                    return Ok(());
                }
            }
            if !seen {
                return Ok(());
            }
        }
    } else if ["mp4", "mov", "avi", "m4v", "wmv", "mkv", "webm"].contains(&extension.as_str()) {
        super::video::play(path, send)?;
    } else {
        let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        reader.limits(limits);
        send(reader.decode()?.to_rgba8(), Duration::ZERO);
    }
    Ok(())
}
