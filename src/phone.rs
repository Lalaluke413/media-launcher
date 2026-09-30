use eframe::egui;
use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct PhoneLink {
    urls: Vec<String>,
    selected: usize,
    checked: Option<Instant>,
    message: String,
    texture: Option<(String, egui::TextureHandle)>,
}

fn urls(address: SocketAddr, ips: impl IntoIterator<Item = IpAddr>) -> Vec<String> {
    if address.ip().is_loopback() {
        return vec![];
    }
    let candidates = if address.ip().is_unspecified() {
        ips.into_iter()
            .filter(|ip| {
                !ip.is_loopback() && !ip.is_unspecified() && ip.is_ipv4() == address.is_ipv4()
            })
            .collect()
    } else {
        vec![address.ip()]
    };
    let mut urls: Vec<_> = candidates
        .into_iter()
        .filter(|ip| match ip {
            IpAddr::V4(ip) => !ip.is_link_local(),
            IpAddr::V6(ip) => ip.segments()[0] & 0xffc0 != 0xfe80,
        })
        .map(|ip| format!("http://{}/", SocketAddr::new(ip, address.port())))
        .collect();
    urls.sort();
    urls.dedup();
    urls
}
impl PhoneLink {
    pub fn refresh(&mut self, endpoint: Option<Result<SocketAddr, String>>) {
        if self
            .checked
            .is_some_and(|last| last.elapsed() < Duration::from_secs(10))
        {
            return;
        }
        let Some(endpoint) = endpoint else {
            self.message = "Starting web UI...".into();
            return;
        };
        self.checked = Some(Instant::now());
        self.urls.clear();
        self.message.clear();
        match endpoint {
            Err(error) => self.message = format!("Web UI unavailable: {error}"),
            Ok(address) if address.ip().is_loopback() => {
                self.message = "Phone access is disabled by the loopback listen address.".into()
            }
            Ok(address) => {
                match if_addrs::get_if_addrs() {
                    Ok(interfaces) => {
                        self.urls = urls(
                            address,
                            interfaces.into_iter().map(|interface| interface.ip()),
                        )
                    }
                    Err(_) if !address.ip().is_unspecified() => self.urls = urls(address, []),
                    Err(error) => {
                        self.message = format!("Could not find a network address: {error}")
                    }
                }
                if self.urls.is_empty() && self.message.is_empty() {
                    self.message =
                        "Connect this computer to your local network to use the phone UI.".into();
                }
            }
        }
        self.selected = self.selected.min(self.urls.len().saturating_sub(1));
    }
    pub fn multiple(&self) -> bool {
        self.urls.len() > 1
    }
    pub fn cycle(&mut self) {
        if self.multiple() {
            self.selected = (self.selected + 1) % self.urls.len();
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Phone web UI").size(20.0).strong());
        let Some(url) = self.urls.get(self.selected) else {
            ui.label(&self.message);
            return;
        };
        if self
            .texture
            .as_ref()
            .is_none_or(|(previous, _)| previous != url)
        {
            let Ok(code) = qrcode::QrCode::new(url.as_bytes()) else {
                ui.label("Could not create QR code.");
                return;
            };
            // Four white modules form the quiet zone required by QR scanners.
            let width = code.width() + 8;
            let mut pixels = vec![egui::Color32::WHITE; width * width];
            for y in 0..code.width() {
                for x in 0..code.width() {
                    if code[(x, y)] == qrcode::Color::Dark {
                        pixels[(y + 4) * width + x + 4] = egui::Color32::BLACK;
                    }
                }
            }
            let texture = ui.ctx().load_texture(
                "phone-qr",
                egui::ColorImage::new([width, width], pixels),
                egui::TextureOptions::NEAREST,
            );
            self.texture = Some((url.clone(), texture));
        }
        let texture = &self.texture.as_ref().unwrap().1;
        let modules = texture.size()[0] as f32;
        let size = modules * (200.0 / modules).floor();
        ui.image((texture.id(), egui::vec2(size, size)));
        ui.hyperlink_to(egui::RichText::new(url).size(14.0), url);
        ui.label(
            egui::RichText::new("Scan to open. Connect your phone to the same network.").size(14.0),
        );
        if self.multiple() {
            ui.small(format!(
                "Address {} of {}",
                self.selected + 1,
                self.urls.len()
            ));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wildcard_uses_concrete_addresses_and_bound_port() {
        let addresses = [
            "127.0.0.1",
            "192.168.1.5",
            "192.168.1.5",
            "::1",
            "169.254.1.2",
        ]
        .map(|ip| ip.parse().unwrap());
        assert_eq!(
            urls("0.0.0.0:4321".parse().unwrap(), addresses),
            ["http://192.168.1.5:4321/"]
        );
    }
    #[test]
    fn explicit_addresses_and_ipv6_are_respected() {
        assert!(urls("127.0.0.1:8765".parse().unwrap(), []).is_empty());
        assert_eq!(
            urls("[fd00::12]:8765".parse().unwrap(), []),
            ["http://[fd00::12]:8765/"]
        );
        assert_eq!(
            urls("192.168.2.4:8765".parse().unwrap(), []),
            ["http://192.168.2.4:8765/"]
        );
    }
}
