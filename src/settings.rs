//! Les réglages, dans une fenêtre à onglets : l'apparence et la lecture,
//! l'envoi vers la Kindle (et l'historique des envois), et « À propos ».

use egui::{Color32, TextureHandle, vec2};

use crate::kindle::{self, Security, Status};
use crate::state::{Paper, State};
use crate::update::Updater;
use crate::widgets::{self, Icon, THEMES, Tone, theme};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    General,
    Kindle,
    About,
}

/// La fenêtre ouverte, et ce qu'on y saisit avant d'enregistrer.
pub struct Window {
    pub tab: Tab,
    /// Les réglages d'envoi en cours de saisie.
    draft: kindle::Settings,
    /// Un nouveau mot de passe (vide : celui du trousseau ne change pas).
    password: String,
    /// Un mot de passe est déjà dans le trousseau pour ce compte.
    has_password: bool,
    /// Le serveur choisi parmi les connus, ou « Autre ».
    provider: Option<usize>,
    /// Ce que l'enregistrement ou l'essai de connexion a donné.
    pub message: Option<Result<String, String>>,
    /// Un essai de connexion est en cours.
    pub testing: bool,
}

impl Window {
    pub fn new(state: &State, tab: Tab) -> Self {
        let draft = state.kindle.clone();
        let provider = kindle::PROVIDERS
            .iter()
            .position(|p| p.1 == draft.server.trim())
            .or_else(|| {
                draft
                    .server
                    .trim()
                    .is_empty()
                    .then(|| kindle::Settings::guess_provider(&draft.from))
                    .flatten()
            });
        Self {
            tab,
            has_password: draft.ready() && kindle::password(&draft).is_some(),
            draft,
            password: String::new(),
            provider,
            message: None,
            testing: false,
        }
    }
}

/// Ce qu'on a fait dans la fenêtre.
#[derive(Default)]
pub struct Outcome {
    pub close: bool,
    /// La mise à jour installée : redémarrer.
    pub relaunch: bool,
    /// Les réglages ont changé : à enregistrer.
    pub changed: bool,
    pub theme: Option<&'static str>,
    pub open_folder: bool,
    /// Essayer la connexion au serveur d'envoi.
    pub test: bool,
    /// Renvoyer un envoi raté de l'historique.
    pub resend: Option<u64>,
}

pub fn show(
    ctx: &egui::Context,
    window: &mut Window,
    state: &mut State,
    updater: &Updater,
    icon: Option<&TextureHandle>,
) -> Outcome {
    let mut out = Outcome::default();
    let width = (ctx.content_rect().width() - 96.0).clamp(320.0, 720.0);
    let frame = egui::Frame::new()
        .fill(theme().background.lerp_to_gamma(theme().surface, 0.6))
        .stroke(egui::Stroke::new(1.0, theme().text.gamma_multiply(0.08)))
        .corner_radius(16)
        .inner_margin(28);
    let modal = egui::Modal::new(egui::Id::new("reglages"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(170))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Réglages")
                        .size(24.0)
                        .strong()
                        .color(theme().text),
                );
                ui.add_space(18.0);
                for (tab, name) in [
                    (Tab::General, "Général"),
                    (Tab::Kindle, "Kindle"),
                    (Tab::About, "À propos"),
                ] {
                    let tone = if window.tab == tab {
                        Tone::Active
                    } else {
                        Tone::Plain
                    };
                    if widgets::button(ui, None, name, tone).clicked() {
                        window.tab = tab;
                    }
                }
            });
            ui.add_space(18.0);
            let room = (ctx.content_rect().height() - 220.0).max(240.0);
            egui::ScrollArea::vertical()
                .max_height(room)
                .min_scrolled_height(room)
                .auto_shrink([false, true])
                .show(ui, |ui| match window.tab {
                    Tab::General => general(ui, state, &mut out),
                    Tab::Kindle => kindle_page(ui, window, state, &mut out),
                    Tab::About => out.relaunch = crate::about::page(ui, updater, icon),
                });
            ui.add_space(14.0);
            if widgets::button(ui, None, "Fermer", Tone::Plain).clicked() {
                out.close = true;
            }
        });
    out.close |= modal.should_close();
    out
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(16.0)
            .strong()
            .color(theme().text),
    );
    ui.add_space(8.0);
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(13.0).color(theme().muted)).wrap());
}

fn general(ui: &mut egui::Ui, state: &mut State, out: &mut Outcome) {
    heading(ui, "Thème");
    ui.horizontal_wrapped(|ui| {
        for t in THEMES {
            let on = t.id == theme().id;
            let tone = if on { Tone::Active } else { Tone::Plain };
            let r = widgets::button(ui, None, &format!("      {}", t.name), tone);
            // La pastille du thème, dans le bouton.
            let c = r.rect.left_center() + vec2(18.0, 0.0);
            ui.painter().circle_filled(c, 8.0, t.background);
            ui.painter().circle_stroke(
                c,
                8.0,
                egui::Stroke::new(1.0, theme().text.gamma_multiply(0.2)),
            );
            ui.painter().circle_filled(c, 4.0, t.accent);
            if r.clicked() && !on {
                out.theme = Some(t.id);
            }
        }
    });

    ui.add_space(24.0);
    heading(ui, "Lecture des BD et mangas");
    let before = (state.panel_zoom, state.panel_dim);
    egui::Grid::new("cases")
        .num_columns(2)
        .spacing([18.0, 12.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Taille des cases").color(theme().muted));
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut state.panel_zoom, false, "À l'échelle de la planche");
                ui.radio_value(&mut state.panel_zoom, true, "Agrandies à l'écran");
            });
            ui.end_row();
            ui.label(egui::RichText::new("Autour de la case").color(theme().muted));
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut state.panel_dim, true, "Planche assombrie");
                ui.radio_value(&mut state.panel_dim, false, "Planche telle quelle");
            });
            ui.end_row();
        });
    out.changed |= (state.panel_zoom, state.panel_dim) != before;
    hint(ui, "Aussi pendant la lecture : E pour la taille, V pour le voile.");

    ui.add_space(24.0);
    heading(ui, "Lecture des livres");
    let before = state.typography;
    let t = &mut state.typography;
    egui::Grid::new("lecture")
        .num_columns(2)
        .spacing([18.0, 12.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Taille du texte").color(theme().muted));
            ui.add(
                egui::Slider::new(&mut t.size, 14.0..=36.0)
                    .step_by(1.0)
                    .fixed_decimals(0)
                    .suffix(" pt"),
            );
            ui.end_row();
            ui.label(egui::RichText::new("Police").color(theme().muted));
            ui.horizontal(|ui| {
                ui.radio_value(&mut t.sans, false, "Avec empattements (PT Serif)");
                ui.radio_value(&mut t.sans, true, "Sans (PT Sans)");
            });
            ui.end_row();
            ui.label(egui::RichText::new("Fond des pages").color(theme().muted));
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(&mut t.paper, Paper::Theme, "Thème");
                ui.radio_value(&mut t.paper, Paper::White, "Blanc");
                ui.radio_value(&mut t.paper, Paper::Sepia, "Sépia");
                ui.radio_value(&mut t.paper, Paper::Night, "Nuit");
            });
            ui.end_row();
        });
    out.changed |= state.typography != before;
    hint(
        ui,
        "Aussi dans la barre du haut pendant la lecture (− et + pour la taille).",
    );

    ui.add_space(24.0);
    heading(ui, "Bibliothèque");
    if let Some(dir) = crate::library::books_dir() {
        hint(
            ui,
            &format!(
                "Serj copie ce que tu ajoutes dans {} (rangé par série, les livres dans « Livres »).",
                dir.display()
            ),
        );
        ui.add_space(6.0);
    }
    out.open_folder |=
        widgets::button(ui, Some(Icon::Folder), "Ouvrir le dossier", Tone::Plain).clicked();
}

fn kindle_page(ui: &mut egui::Ui, window: &mut Window, state: &mut State, out: &mut Outcome) {
    heading(ui, "Envoyer vers ma Kindle");
    hint(
        ui,
        "Serj envoie le livre par e-mail à ta Kindle, depuis ton adresse. Dans ton compte \
         Amazon (Préférences, puis Paramètres des documents personnels), ajoute ton adresse \
         d'envoi aux e-mails approuvés ; l'adresse de ta Kindle y est aussi. \
         Amazon accepte les EPUB, PDF et TXT, jusqu'à 50 Mo.",
    );
    ui.hyperlink_to("Ouvrir les réglages Amazon", kindle::AMAZON_SETTINGS);
    ui.add_space(14.0);

    let d = &mut window.draft;
    let field_w = 320.0;
    egui::Grid::new("kindle")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Adresse de la Kindle").color(theme().muted));
            ui.add(
                egui::TextEdit::singleline(&mut d.kindle)
                    .hint_text("prenom_123@kindle.com")
                    .desired_width(field_w),
            );
            ui.end_row();
            ui.label(egui::RichText::new("Ton adresse d'envoi").color(theme().muted));
            let from = ui.add(
                egui::TextEdit::singleline(&mut d.from)
                    .hint_text("toi@gmail.com")
                    .desired_width(field_w),
            );
            // Une adresse Gmail ou iCloud choisit son serveur toute seule.
            if from.changed()
                && let Some(p) = kindle::Settings::guess_provider(&d.from)
            {
                window.provider = Some(p);
            }
            ui.end_row();
            ui.label(egui::RichText::new("Messagerie").color(theme().muted));
            ui.horizontal(|ui| {
                for (i, p) in kindle::PROVIDERS.iter().enumerate() {
                    ui.radio_value(&mut window.provider, Some(i), p.0);
                }
                ui.radio_value(&mut window.provider, None, "Autre");
            });
            ui.end_row();
            match window.provider {
                Some(i) => {
                    let (_, server, port, security, _) = kindle::PROVIDERS[i];
                    d.server = server.into();
                    d.port = port;
                    d.security = security;
                }
                None => {
                    ui.label(egui::RichText::new("Serveur SMTP").color(theme().muted));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut d.server)
                                .hint_text("smtp.exemple.fr")
                                .desired_width(field_w - 90.0),
                        );
                        ui.label(egui::RichText::new("Port").color(theme().muted));
                        let mut port = if d.port == 0 {
                            String::new()
                        } else {
                            d.port.to_string()
                        };
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut port)
                                    .hint_text("587")
                                    .desired_width(48.0),
                            )
                            .changed()
                        {
                            d.port = port.trim().parse().unwrap_or(0);
                        }
                    });
                    ui.end_row();
                    ui.label(egui::RichText::new("Sécurité").color(theme().muted));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut d.security, Security::StartTls, "STARTTLS");
                        ui.radio_value(&mut d.security, Security::Tls, "SSL / TLS");
                        ui.radio_value(&mut d.security, Security::None, "Aucune")
                            .on_hover_text("Seulement pour un serveur local, pour essayer");
                    });
                    ui.end_row();
                    ui.label(egui::RichText::new("Identifiant").color(theme().muted));
                    ui.add(
                        egui::TextEdit::singleline(&mut d.login)
                            .hint_text("ton adresse d'envoi, le plus souvent")
                            .desired_width(field_w),
                    );
                    ui.end_row();
                }
            }
            ui.label(egui::RichText::new("Mot de passe").color(theme().muted));
            ui.horizontal(|ui| {
                let hint = if window.has_password {
                    "enregistré dans le trousseau"
                } else {
                    "mot de passe d'application"
                };
                ui.add(
                    egui::TextEdit::singleline(&mut window.password)
                        .password(true)
                        .hint_text(hint)
                        .desired_width(field_w),
                );
                if let Some(i) = window.provider {
                    ui.hyperlink_to("En créer un", kindle::PROVIDERS[i].4);
                }
            });
            ui.end_row();
        });
    if d.port == 0 {
        d.port = match d.security {
            Security::Tls => 465,
            _ => 587,
        };
    }
    if window.provider.is_some() {
        ui.add_space(4.0);
        hint(
            ui,
            "Gmail et iCloud demandent un mot de passe d'application (pas celui du compte), \
             créé dans les réglages de sécurité du compte, avec la validation en deux étapes.",
        );
    }

    ui.add_space(12.0);
    let mut save = false;
    ui.horizontal(|ui| {
        save = widgets::button(ui, None, "Enregistrer", Tone::Primary).clicked();
        let test = widgets::button(ui, None, "Tester la connexion", Tone::Plain)
            .on_hover_text("Se connecter au serveur avec ce compte, sans rien envoyer")
            .clicked();
        if window.has_password
            && widgets::button(ui, None, "Oublier le mot de passe", Tone::Plain).clicked()
        {
            kindle::forget_password(&state.kindle);
            window.has_password = false;
            window.message = Some(Ok("Mot de passe retiré du trousseau.".into()));
        }
        if test {
            save = true;
            out.test = true;
        }
        if window.testing {
            ui.spinner();
        }
    });
    if save {
        let mut result = Ok("Réglages enregistrés.".to_string());
        if !window.password.is_empty() {
            match kindle::set_password(&window.draft, &window.password) {
                Ok(()) => {
                    window.has_password = true;
                    window.password.clear();
                }
                Err(e) => result = Err(e),
            }
        } else if window.draft.login() != state.kindle.login()
            || window.draft.server.trim() != state.kindle.server.trim()
        {
            window.has_password = kindle::password(&window.draft).is_some();
        }
        state.kindle = window.draft.clone();
        out.changed = true;
        if !state.kindle.ready() {
            result = Err("Il manque l'adresse de la Kindle, celle d'envoi ou le serveur.".into());
            out.test = false;
        }
        if out.test {
            window.testing = true;
            window.message = None;
        } else {
            window.message = Some(result);
        }
    }
    if let Some(message) = &window.message {
        ui.add_space(6.0);
        let (text, color) = match message {
            Ok(t) => (t.as_str(), theme().accent),
            Err(e) => (e.as_str(), Color32::from_rgb(0xE5, 0x5B, 0x4F)),
        };
        ui.add(egui::Label::new(egui::RichText::new(text).color(color)).wrap());
    }

    ui.add_space(26.0);
    ui.horizontal(|ui| {
        heading(ui, "Historique des envois");
        if !state.sent.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, None, "Effacer l'historique", Tone::Plain).clicked() {
                    state.sent.retain(|s| s.status == Status::Sending);
                    out.changed = true;
                }
            });
        }
    });
    if state.sent.is_empty() {
        hint(
            ui,
            "Aucun envoi pour l'instant. Clic droit sur un livre de la bibliothèque, \
             ou sa fiche : « Envoyer vers ma Kindle ».",
        );
        return;
    }
    let now = kindle::now();
    for sent in &state.sent {
        egui::Frame::new()
            .fill(theme().surface)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                    match &sent.status {
                        Status::Sending => {
                            ui.put(r, egui::Spinner::new().size(14.0));
                        }
                        Status::Sent => {
                            ui.painter().circle_filled(
                                r.center(),
                                5.0,
                                Color32::from_rgb(0x4C, 0xC3, 0x8A),
                            );
                        }
                        Status::Failed(_) => {
                            ui.painter().circle_filled(
                                r.center(),
                                5.0,
                                Color32::from_rgb(0xE5, 0x5B, 0x4F),
                            );
                        }
                    }
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&sent.title)
                                    .strong()
                                    .color(theme().text),
                            )
                            .truncate(),
                        );
                        let status = match &sent.status {
                            Status::Sending => "envoi en cours…".to_string(),
                            Status::Sent => "envoyé".to_string(),
                            Status::Failed(e) => format!("échec : {e}"),
                        };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{}   ·   vers {}   ·   {status}",
                                    kindle::ago(sent.at, now),
                                    sent.to
                                ))
                                .size(12.5)
                                .color(theme().muted),
                            )
                            .wrap(),
                        );
                    });
                    if matches!(sent.status, Status::Failed(_)) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::button(ui, None, "Renvoyer", Tone::Plain).clicked() {
                                out.resend = Some(sent.id);
                            }
                        });
                    }
                });
            });
        ui.add_space(6.0);
    }
}
