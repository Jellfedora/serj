//! La fenêtre « À propos » : la version, les mises à jour, les liens du
//! projet, les raccourcis et les nouveautés de chaque version.

use egui::{Color32, TextureHandle, vec2};

use crate::update::{self, REPO, Updater, VERSION};
use crate::widgets::{self, Icon, Tone, theme};

const CHANGELOG: &str = include_str!("../CHANGELOG.md");
const README: &str = include_str!("../README.md");

/// Ce qu'on a fait dans la fenêtre.
#[derive(Default)]
pub struct Outcome {
    pub close: bool,
    /// La mise à jour installée : redémarrer.
    pub relaunch: bool,
}

pub fn show(ctx: &egui::Context, updater: &Updater, icon: Option<&TextureHandle>) -> Outcome {
    let mut out = Outcome::default();
    let width = (ctx.content_rect().width() - 96.0).clamp(300.0, 680.0);
    let frame = egui::Frame::new()
        .fill(theme().background.lerp_to_gamma(theme().surface, 0.6))
        .stroke(egui::Stroke::new(1.0, theme().text.gamma_multiply(0.08)))
        .corner_radius(16)
        .inner_margin(28);
    let modal = egui::Modal::new(egui::Id::new("a-propos"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(170))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                if let Some(icon) = icon {
                    ui.add(egui::Image::from_texture((icon.id(), vec2(64.0, 64.0))));
                    ui.add_space(8.0);
                }
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Serj").size(28.0).strong().color(theme().text));
                    ui.label(
                        egui::RichText::new(format!("Version {VERSION}   ·   Une case à la fois."))
                            .size(13.0)
                            .color(theme().muted),
                    );
                });
            });

            ui.add_space(16.0);
            updates(ui, ctx, updater, &mut out);

            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let repo = format!("https://github.com/{REPO}");
                ui.hyperlink_to("Code source sur GitHub", &repo);
                ui.hyperlink_to("Toutes les versions", format!("{repo}/releases"));
                ui.hyperlink_to("Signaler un problème", format!("{repo}/issues/new"));
            });

            ui.add_space(18.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 380.0).max(180.0))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    heading(ui, "Raccourcis");
                    egui::Grid::new("raccourcis")
                        .num_columns(2)
                        .spacing([18.0, 6.0])
                        .show(ui, |ui| {
                            for (keys, action) in shortcuts() {
                                ui.label(egui::RichText::new(keys).strong().color(theme().text));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(action).color(theme().muted),
                                    )
                                    .wrap(),
                                );
                                ui.end_row();
                            }
                        });
                    ui.add_space(18.0);
                    for (version, items) in changelog() {
                        let title = if version == VERSION {
                            format!("Nouveautés de la {version} (cette version)")
                        } else {
                            format!("Nouveautés de la {version}")
                        };
                        heading(ui, &title);
                        for item in items {
                            ui.horizontal_top(|ui| {
                                ui.label(egui::RichText::new("•").color(theme().accent));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(item)
                                            .size(13.5)
                                            .color(theme().text.gamma_multiply(0.85)),
                                    )
                                    .wrap(),
                                );
                            });
                        }
                        ui.add_space(14.0);
                    }
                });

            ui.add_space(12.0);
            if widgets::button(ui, None, "Fermer", Tone::Plain).clicked() {
                out.close = true;
            }
        });
    out.close |= modal.should_close();
    out
}

/// Où en sont les mises à jour, et de quoi en chercher ou en installer une.
fn updates(ui: &mut egui::Ui, ctx: &egui::Context, updater: &Updater, out: &mut Outcome) {
    let t = theme();
    egui::Frame::new()
        .fill(t.surface)
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let row = vec2(ui.available_width(), 34.0);
            let centered = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(row, centered, |ui| {
                let state = updater.state();
                let text = match &state {
                    update::State::Idle => "Mises à jour".to_string(),
                    update::State::Checking => "Recherche d'une nouvelle version…".to_string(),
                    update::State::UpToDate => "Serj est à jour.".to_string(),
                    update::State::Available(a) => format!("Serj {} est disponible.", a.version),
                    update::State::Installing(v) => format!("Installation de Serj {v}…"),
                    update::State::Installed(v) => {
                        format!("Serj {v} est installé : redémarre pour en profiter.")
                    }
                    update::State::Failed(e) => format!("Échec : {e}"),
                };
                if matches!(state, update::State::Checking | update::State::Installing(_)) {
                    ui.spinner();
                    ctx.request_repaint_after(std::time::Duration::from_millis(150));
                }
                ui.add(egui::Label::new(egui::RichText::new(text).color(t.text)).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match &state {
                        update::State::Idle
                        | update::State::UpToDate
                        | update::State::Failed(_) => {
                            if widgets::button(ui, None, "Vérifier les mises à jour", Tone::Plain)
                                .clicked()
                            {
                                updater.check(ctx);
                            }
                        }
                        update::State::Available(a) => {
                            if updater.can_install() {
                                if widgets::button(
                                    ui,
                                    Some(Icon::Download),
                                    "Mettre à jour",
                                    Tone::Primary,
                                )
                                .clicked()
                                {
                                    updater.install(ctx, a.clone());
                                }
                            } else if widgets::button(
                                ui,
                                Some(Icon::Download),
                                "Télécharger",
                                Tone::Primary,
                            )
                            .clicked()
                            {
                                ctx.open_url(egui::OpenUrl::new_tab(&a.url));
                            }
                        }
                        update::State::Installed(_) => {
                            if widgets::button(ui, None, "Redémarrer", Tone::Primary).clicked() {
                                out.relaunch = true;
                            }
                        }
                        update::State::Checking | update::State::Installing(_) => {}
                    }
                });
            });
        });
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(15.0).strong().color(theme().text));
    ui.add_space(6.0);
}

/// Les raccourcis, tirés du tableau du README (les flèches en toutes lettres :
/// la police n'en a pas).
fn shortcuts() -> Vec<(String, &'static str)> {
    README
        .split("## Raccourcis")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .unwrap_or("")
        .lines()
        .filter_map(|l| {
            let cells: Vec<&str> = l.trim().trim_matches('|').split('|').map(str::trim).collect();
            match cells.as_slice() {
                [keys, action] if *keys != "Touche" && !keys.starts_with("---") => {
                    let keys = keys
                        .replace("→ ↓", "Droite, bas,")
                        .replace("← ↑", "Gauche, haut");
                    Some((keys, *action))
                }
                _ => None,
            }
        })
        .collect()
}

/// Les versions publiées du CHANGELOG (sans « Unreleased »), la plus récente
/// d'abord, avec leurs nouveautés.
fn changelog() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut versions: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in CHANGELOG.lines() {
        if let Some(v) = line.strip_prefix("## ") {
            versions.push((v.trim().trim_matches(['[', ']']), Vec::new()));
        } else if let Some(item) = line.strip_prefix("- ")
            && let Some((_, items)) = versions.last_mut()
        {
            items.push(item.trim());
        }
    }
    versions.retain(|(v, _)| *v != "Unreleased");
    versions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shortcuts_and_changelog() {
        let keys = shortcuts();
        assert!(keys.iter().any(|(k, _)| k == "F"));
        assert!(!keys.iter().any(|(k, _)| k == "Touche" || k.contains('→')));
        let versions = changelog();
        assert!(versions.iter().any(|(v, items)| *v == "0.3.0" && !items.is_empty()));
    }
}
