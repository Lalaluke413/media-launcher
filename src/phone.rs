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

// An address-only heuristic: private ranges can also belong to VPNs or virtual
// adapters, so this order cannot guarantee that a phone can reach the address.
fn lan_priority(ip: IpAddr) -> u8 {
    match ip {
        IpAddr::V4(ip) => match ip.octets() {
            [192, 168, _, _] => 0,
            [10, _, _, _] => 1,
            [172, 16..=31, _, _] => 2,
            _ => 3,
        },
        IpAddr::V6(ip) => {
            if ip.is_unique_local() {
                0
            } else {
                3
            }
        }
    }
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
    let mut candidates: Vec<_> = candidates
        .into_iter()
        .filter(|ip| match ip {
            IpAddr::V4(ip) => !ip.is_link_local(),
            IpAddr::V6(ip) => ip.segments()[0] & 0xffc0 != 0xfe80,
        })
        .collect();
    candidates.sort_unstable_by_key(|ip| (lan_priority(*ip), *ip));
    candidates.dedup();
    candidates
        .into_iter()
        .map(|ip| format!("http://{}/", SocketAddr::new(ip, address.port())))
        .collect()
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
        let previous = self.address().map(str::to_owned);
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
        self.selected = previous
            .and_then(|url| self.urls.iter().position(|candidate| candidate == &url))
            .unwrap_or_else(|| self.selected.min(self.urls.len().saturating_sub(1)));
    }
    pub fn address(&self) -> Option<&str> {
        self.urls.get(self.selected).map(String::as_str)
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
        self.show_sized(ui, 200.0);
    }
    pub fn show_sized(&mut self, ui: &mut egui::Ui, qr_size: f32) {
        self.show_header(ui, qr_size);
    }
    pub fn show_header(&mut self, ui: &mut egui::Ui, qr_size: f32) {
        let Some(url) = self.urls.get(self.selected) else {
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
        let size = modules
            * (qr_size.min(ui.available_width()) / modules)
                .floor()
                .max(1.0);
        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
            ui.image((texture.id(), egui::vec2(size, size)));
            ui.add(egui::Label::new(egui::RichText::new(url).size(18.0)).truncate());
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_qr_and_address_reach_the_right_edge() {
        let context = egui::Context::default();
        let mut phone = PhoneLink {
            urls: vec!["http://192.168.1.2:8080/".into()],
            ..Default::default()
        };
        let mut right = 0.0;
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::TopBottomPanel::top("header")
                    .exact_height(260.0)
                    .show(ctx, |ui| {
                        right = ui.max_rect().right();
                        ui.add_space(12.0);
                        crate::header_columns(
                            ui,
                            236.0,
                            |ui| {
                                ui.label(".");
                                ui.label("video.mkv");
                                ui.label("1.00 GB");
                            },
                            |ui| {
                                phone.show_header(ui, 200.0);
                            },
                        );
                    });
            },
        );
        fn flatten<'a>(shape: &'a egui::epaint::Shape, shapes: &mut Vec<&'a egui::epaint::Shape>) {
            if let egui::epaint::Shape::Vec(children) = shape {
                for child in children {
                    flatten(child, shapes);
                }
            } else {
                shapes.push(shape);
            }
        }
        let mut shapes = vec![];
        for shape in &output.shapes {
            flatten(&shape.shape, &mut shapes);
        }
        let texture = phone.texture.as_ref().unwrap().1.id();
        let qr = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::epaint::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    Some(mesh.calc_bounds())
                }
                egui::epaint::Shape::Rect(rect)
                    if rect
                        .brush
                        .as_ref()
                        .is_some_and(|brush| brush.fill_texture_id == texture) =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("QR image painted");
        assert!(
            (qr.right() - right).abs() < 1.0,
            "QR right {} != header right {right}",
            qr.right()
        );
        let address = shapes
            .iter()
            .find_map(|shape| match shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.text() == "http://192.168.1.2:8080/" =>
                {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            })
            .expect("address painted");
        assert!(
            (address.right() - right).abs() < 1.0,
            "address right {} != header right {right}",
            address.right()
        );
        assert!(address.top() >= qr.bottom());
    }
    #[test]
    fn wildcard_prefers_lan_ranges_with_numeric_ties() {
        let addresses = [
            "172.32.0.1",
            "10.0.0.2",
            "192.168.1.100",
            "172.31.255.254",
            "192.168.1.20",
            "172.16.0.1",
            "172.15.255.254",
            "192.168.1.20",
        ]
        .map(|ip| ip.parse().unwrap());
        let expected = [
            "192.168.1.20",
            "192.168.1.100",
            "10.0.0.2",
            "172.16.0.1",
            "172.31.255.254",
            "172.15.255.254",
            "172.32.0.1",
        ]
        .map(|ip| format!("http://{ip}:4321/"));
        assert_eq!(urls("0.0.0.0:4321".parse().unwrap(), addresses), expected);
        assert_eq!(
            urls("0.0.0.0:4321".parse().unwrap(), addresses.into_iter().rev()),
            expected
        );
        assert_eq!(
            urls("10.0.0.2:4321".parse().unwrap(), addresses),
            ["http://10.0.0.2:4321/"]
        );
    }
    #[test]
    fn wildcard_ipv6_prefers_unique_local_addresses() {
        let addresses = [
            "2001:db8::1",
            "fd00::12",
            "fc00::2",
            "fe80::1",
            "::1",
            "::",
            "192.168.1.2",
        ]
        .map(|ip| ip.parse().unwrap());
        assert_eq!(
            urls("[::]:4321".parse().unwrap(), addresses),
            [
                "http://[fc00::2]:4321/",
                "http://[fd00::12]:4321/",
                "http://[2001:db8::1]:4321/",
            ]
        );
    }
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
