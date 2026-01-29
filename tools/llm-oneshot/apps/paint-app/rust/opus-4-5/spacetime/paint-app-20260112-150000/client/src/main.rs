mod module_bindings;

use module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity, Table, TableWithPrimaryKey};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use std::sync::{Arc, Mutex};

const SPACETIMEDB_URI: &str = "http://127.0.0.1:3000";
const MODULE_NAME: &str = "paint-app-rust";

// SpacetimeDB cosmic dark theme colors
const BG_COLOR: Color32 = Color32::from_rgb(10, 10, 15);
const PANEL_BG: Color32 = Color32::from_rgb(20, 20, 30);
const ACCENT_PURPLE: Color32 = Color32::from_rgb(99, 102, 241);
const ACCENT_CYAN: Color32 = Color32::from_rgb(34, 211, 238);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(228, 228, 231);
const TEXT_DIM: Color32 = Color32::from_rgb(113, 113, 122);

#[derive(Clone, PartialEq)]
enum Tool {
    Select,
    Brush,
    Eraser,
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Text,
    Sticky,
}

impl Tool {
    fn as_str(&self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Brush => "brush",
            Tool::Eraser => "eraser",
            Tool::Rectangle => "rectangle",
            Tool::Ellipse => "ellipse",
            Tool::Line => "line",
            Tool::Arrow => "arrow",
            Tool::Text => "text",
            Tool::Sticky => "sticky",
        }
    }
    
    fn icon(&self) -> &'static str {
        match self {
            Tool::Select => "[V]",
            Tool::Brush => "[B]",
            Tool::Eraser => "[E]",
            Tool::Rectangle => "[R]",
            Tool::Ellipse => "[O]",
            Tool::Line => "[L]",
            Tool::Arrow => "[A]",
            Tool::Text => "[T]",
            Tool::Sticky => "[S]",
        }
    }
}

#[derive(Clone)]
struct AppState {
    status: String,
    my_identity: Option<Identity>,
    users: Vec<User>,
    canvases: Vec<Canvas>,
    canvas_members: Vec<CanvasMember>,
    strokes: Vec<module_bindings::Stroke>,
    elements: Vec<Element>,
    layers: Vec<Layer>,
    user_selections: Vec<UserSelection>,
    comments: Vec<Comment>,
    chat_messages: Vec<ChatMessage>,
    activity_entries: Vec<ActivityEntry>,
    typing_indicators: Vec<TypingIndicator>,
    notifications: Vec<Notification>,
    
    // Local UI state
    my_name_input: String,
    current_tool: Tool,
    brush_color: Color32,
    fill_color: Color32,
    brush_size: f32,
    is_drawing: bool,
    current_stroke_points: Vec<Pos2>,
    drag_start: Option<Pos2>,
    selected_element_ids: Vec<u64>,
    
    // Panels
    show_layers_panel: bool,
    show_chat_panel: bool,
    show_activity_panel: bool,
    show_users_panel: bool,
    
    // Chat
    chat_input: String,
    
    // Shape preview
    shape_preview: Option<(Pos2, Pos2)>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            status: "Connecting...".to_string(),
            my_identity: None,
            users: vec![],
            canvases: vec![],
            canvas_members: vec![],
            strokes: vec![],
            elements: vec![],
            layers: vec![],
            user_selections: vec![],
            comments: vec![],
            chat_messages: vec![],
            activity_entries: vec![],
            typing_indicators: vec![],
            notifications: vec![],
            my_name_input: String::new(),
            current_tool: Tool::Brush,
            brush_color: ACCENT_PURPLE,
            fill_color: Color32::TRANSPARENT,
            brush_size: 5.0,
            is_drawing: false,
            current_stroke_points: vec![],
            drag_start: None,
            selected_element_ids: vec![],
            show_layers_panel: true,
            show_chat_panel: false,
            show_activity_panel: false,
            show_users_panel: true,
            chat_input: String::new(),
            shape_preview: None,
        }
    }
}

type SharedState = Arc<Mutex<AppState>>;
type SharedConnection = Arc<Mutex<Option<DbConnection>>>;

fn load_token() -> Option<String> {
    let path = dirs::data_local_dir()?.join("paint-client-token.txt");
    std::fs::read_to_string(path).ok()
}

fn save_token(token: &str) {
    if let Some(dir) = dirs::data_local_dir() {
        let path = dir.join("paint-client-token.txt");
        let _ = std::fs::write(path, token);
    }
}

fn main() -> eframe::Result<()> {
    let state = Arc::new(Mutex::new(AppState::default()));
    let connection: SharedConnection = Arc::new(Mutex::new(None));
    
    let state_for_conn = state.clone();
    let conn_for_thread = connection.clone();
    
    std::thread::spawn(move || {
        connect_to_spacetimedb(state_for_conn, conn_for_thread);
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Paint App - SpacetimeDB"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Paint App",
        options,
        Box::new(|_cc| Ok(Box::new(PaintApp { state, connection }))),
    )
}

fn connect_to_spacetimedb(state: SharedState, connection: SharedConnection) {
    let state_connect = state.clone();
    let state_disconnect = state.clone();
    let state_error = state.clone();
    let conn_store = connection.clone();

    let result = DbConnection::builder()
        .with_uri(SPACETIMEDB_URI)
        .with_module_name(MODULE_NAME)
        .with_token(load_token())
        .on_connect(move |conn, identity, token| {
            println!("Connected: {:?}", identity);
            save_token(token);
            
            {
                let mut s = state_connect.lock().unwrap();
                s.my_identity = Some(identity);
                s.status = "Subscribing...".to_string();
            }

            let state_sub = state_connect.clone();
            conn.subscription_builder()
                .on_applied(move |ctx| {
                    println!("Subscriptions applied");
                    let mut s = state_sub.lock().unwrap();
                    s.status = "Ready".to_string();
                    s.users = ctx.db.user().iter().collect();
                    s.canvases = ctx.db.canvas().iter().collect();
                    s.canvas_members = ctx.db.canvas_member().iter().collect();
                    s.strokes = ctx.db.stroke().iter().collect();
                    s.elements = ctx.db.element().iter().collect();
                    s.layers = ctx.db.layer().iter().collect();
                    s.user_selections = ctx.db.user_selection().iter().collect();
                    s.comments = ctx.db.comment().iter().collect();
                    s.chat_messages = ctx.db.chat_message().iter().collect();
                    s.activity_entries = ctx.db.activity_entry().iter().collect();
                    s.typing_indicators = ctx.db.typing_indicator().iter().collect();
                    s.notifications = ctx.db.notification().iter().collect();
                })
                .subscribe([
                    "SELECT * FROM user".to_string(),
                    "SELECT * FROM canvas".to_string(),
                    "SELECT * FROM canvas_member".to_string(),
                    "SELECT * FROM stroke".to_string(),
                    "SELECT * FROM element".to_string(),
                    "SELECT * FROM layer".to_string(),
                    "SELECT * FROM user_selection".to_string(),
                    "SELECT * FROM comment".to_string(),
                    "SELECT * FROM chat_message".to_string(),
                    "SELECT * FROM activity_entry".to_string(),
                    "SELECT * FROM typing_indicator".to_string(),
                    "SELECT * FROM notification".to_string(),
                ]);

            setup_callbacks(&conn, state_connect.clone());
        })
        .on_disconnect(move |_ctx, err| {
            println!("Disconnected: {:?}", err);
            let mut s = state_disconnect.lock().unwrap();
            s.status = "Disconnected".to_string();
        })
        .on_connect_error(move |_ctx, err| {
            println!("Connection error: {:?}", err);
            let mut s = state_error.lock().unwrap();
            s.status = format!("Error: {:?}", err);
        })
        .build();

    match result {
        Ok(conn) => {
            conn.run_threaded();
            *conn_store.lock().unwrap() = Some(conn);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        Err(e) => {
            let mut s = state.lock().unwrap();
            s.status = format!("Failed to connect: {:?}", e);
        }
    }
}

fn setup_callbacks(conn: &DbConnection, state: SharedState) {
    // User callbacks
    let s = state.clone();
    conn.db.user().on_insert(move |_ctx, user| {
        let mut state = s.lock().unwrap();
        state.users.push(user.clone());
    });
    
    let s = state.clone();
    conn.db.user().on_update(move |_ctx, _old, new| {
        let mut state = s.lock().unwrap();
        if let Some(idx) = state.users.iter().position(|u| u.identity == new.identity) {
            state.users[idx] = new.clone();
        }
    });
    
    let s = state.clone();
    conn.db.user().on_delete(move |_ctx, old| {
        let mut state = s.lock().unwrap();
        state.users.retain(|u| u.identity != old.identity);
    });

    // Canvas callbacks
    let s = state.clone();
    conn.db.canvas().on_insert(move |_ctx, canvas| {
        let mut state = s.lock().unwrap();
        state.canvases.push(canvas.clone());
    });
    
    let s = state.clone();
    conn.db.canvas().on_update(move |_ctx, _old, new| {
        let mut state = s.lock().unwrap();
        if let Some(idx) = state.canvases.iter().position(|c| c.id == new.id) {
            state.canvases[idx] = new.clone();
        }
    });
    
    let s = state.clone();
    conn.db.canvas().on_delete(move |_ctx, old| {
        let mut state = s.lock().unwrap();
        state.canvases.retain(|c| c.id != old.id);
    });

    // Canvas member callbacks
    let s = state.clone();
    conn.db.canvas_member().on_insert(move |_ctx, member| {
        let mut state = s.lock().unwrap();
        state.canvas_members.push(member.clone());
    });
    
    let s = state.clone();
    conn.db.canvas_member().on_delete(move |_ctx, old| {
        let mut state = s.lock().unwrap();
        state.canvas_members.retain(|m| m.id != old.id);
    });

    // Stroke callbacks
    let s = state.clone();
    conn.db.stroke().on_insert(move |_ctx, stroke| {
        let mut state = s.lock().unwrap();
        state.strokes.push(stroke.clone());
    });
    
    let s = state.clone();
    conn.db.stroke().on_delete(move |_ctx, old| {
        let mut state = s.lock().unwrap();
        state.strokes.retain(|st| st.id != old.id);
    });

    // Element callbacks
    let s = state.clone();
    conn.db.element().on_insert(move |_ctx, element| {
        let mut state = s.lock().unwrap();
        state.elements.push(element.clone());
    });
    
    let s = state.clone();
    conn.db.element().on_update(move |_ctx, _old, new| {
        let mut state = s.lock().unwrap();
        if let Some(idx) = state.elements.iter().position(|e| e.id == new.id) {
            state.elements[idx] = new.clone();
        }
    });
    
    let s = state.clone();
    conn.db.element().on_delete(move |_ctx, old| {
        let mut state = s.lock().unwrap();
        state.elements.retain(|e| e.id != old.id);
    });

    // Layer callbacks
    let s = state.clone();
    conn.db.layer().on_insert(move |_ctx, layer| {
        let mut state = s.lock().unwrap();
        state.layers.push(layer.clone());
    });
    
    let s = state.clone();
    conn.db.layer().on_update(move |_ctx, _old, new| {
        let mut state = s.lock().unwrap();
        if let Some(idx) = state.layers.iter().position(|l| l.id == new.id) {
            state.layers[idx] = new.clone();
        }
    });
    
    let s = state.clone();
    conn.db.layer().on_delete(move |_ctx, old| {
        let mut state = s.lock().unwrap();
        state.layers.retain(|l| l.id != old.id);
    });

    // Chat message callbacks
    let s = state.clone();
    conn.db.chat_message().on_insert(move |_ctx, msg| {
        let mut state = s.lock().unwrap();
        state.chat_messages.push(msg.clone());
    });

    // Activity entry callbacks  
    let s = state.clone();
    conn.db.activity_entry().on_insert(move |_ctx, entry| {
        let mut state = s.lock().unwrap();
        state.activity_entries.push(entry.clone());
        if state.activity_entries.len() > 100 {
            state.activity_entries.remove(0);
        }
    });

    // User selection callbacks
    let s = state.clone();
    conn.db.user_selection().on_insert(move |_ctx, sel| {
        let mut state = s.lock().unwrap();
        state.user_selections.push(sel.clone());
    });
    
    let s = state.clone();
    conn.db.user_selection().on_update(move |_ctx, _old, new| {
        let mut state = s.lock().unwrap();
        if let Some(idx) = state.user_selections.iter().position(|s| s.id == new.id) {
            state.user_selections[idx] = new.clone();
        }
    });
    
    let s = state.clone();
    conn.db.user_selection().on_delete(move |_ctx, old| {
        let mut state = s.lock().unwrap();
        state.user_selections.retain(|sel| sel.id != old.id);
    });
}

struct PaintApp {
    state: SharedState,
    connection: SharedConnection,
}

impl PaintApp {
    fn call_reducer<F>(&self, f: F)
    where
        F: FnOnce(&DbConnection),
    {
        if let Some(conn) = self.connection.lock().unwrap().as_ref() {
            f(conn);
        }
    }
    
    fn get_my_user<'a>(&self, state: &'a AppState) -> Option<&'a User> {
        state.users.iter().find(|u| {
            state.my_identity.as_ref().map_or(false, |id| u.identity == *id)
        })
    }
    
    fn am_i_member_of(&self, state: &AppState, canvas_id: u64) -> bool {
        state.canvas_members.iter().any(|m| {
            m.canvas_id == canvas_id && 
            state.my_identity.as_ref().map_or(false, |id| m.user_identity == *id)
        })
    }
}

impl eframe::App for PaintApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        
        // Apply cosmic dark theme
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = PANEL_BG;
        visuals.window_fill = PANEL_BG;
        visuals.widgets.noninteractive.bg_fill = BG_COLOR;
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 30, 45);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 40, 60);
        visuals.widgets.active.bg_fill = ACCENT_PURPLE;
        visuals.selection.bg_fill = ACCENT_PURPLE;
        ctx.set_visuals(visuals);

        let state = self.state.lock().unwrap().clone();
        
        self.handle_keyboard_shortcuts(ctx);

        match state.status.as_str() {
            "Ready" => {
                let my_user = self.get_my_user(&state);

                // Check if user needs to set name
                if my_user.map_or(true, |u| u.name.is_empty() || u.name.starts_with("User-")) {
                    self.show_name_screen(ctx);
                } 
                // Check if user is on a canvas (from server state, not local)
                else if let Some(canvas_id) = my_user.and_then(|u| u.current_canvas_id) {
                    self.show_paint_view(ctx, &state, canvas_id);
                } else {
                    self.show_canvas_list(ctx, &state);
                }
            }
            _ => {
                self.show_loading(ctx, &state.status);
            }
        }
    }
}

impl PaintApp {
    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::V) {
                self.state.lock().unwrap().current_tool = Tool::Select;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::B) {
                self.state.lock().unwrap().current_tool = Tool::Brush;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::E) {
                self.state.lock().unwrap().current_tool = Tool::Eraser;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::R) {
                self.state.lock().unwrap().current_tool = Tool::Rectangle;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::O) {
                self.state.lock().unwrap().current_tool = Tool::Ellipse;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::L) {
                self.state.lock().unwrap().current_tool = Tool::Line;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::A) {
                self.state.lock().unwrap().current_tool = Tool::Arrow;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::T) {
                self.state.lock().unwrap().current_tool = Tool::Text;
                self.update_tool_on_server();
            }
            if i.key_pressed(egui::Key::S) && !i.modifiers.ctrl {
                self.state.lock().unwrap().current_tool = Tool::Sticky;
                self.update_tool_on_server();
            }
            
            if i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace) {
                self.delete_selected_elements();
            }
            
            if i.key_pressed(egui::Key::Escape) {
                self.deselect_all();
            }
        });
    }
    
    fn update_tool_on_server(&self) {
        let tool = self.state.lock().unwrap().current_tool.clone();
        self.call_reducer(|conn| {
            let _ = conn.reducers.set_tool(tool.as_str().to_string());
        });
    }
    
    fn delete_selected_elements(&self) {
        let ids = self.state.lock().unwrap().selected_element_ids.clone();
        for id in ids {
            self.call_reducer(|conn| {
                let _ = conn.reducers.delete_element(id);
            });
        }
        self.state.lock().unwrap().selected_element_ids.clear();
    }
    
    fn deselect_all(&self) {
        let state = self.state.lock().unwrap();
        let my_user = state.users.iter().find(|u| {
            state.my_identity.as_ref().map_or(false, |id| u.identity == *id)
        });
        if let Some(canvas_id) = my_user.and_then(|u| u.current_canvas_id) {
            drop(state);
            self.state.lock().unwrap().selected_element_ids.clear();
            self.call_reducer(|conn| {
                let _ = conn.reducers.deselect_all(canvas_id);
            });
        }
    }

    fn show_loading(&self, ctx: &egui::Context, status: &str) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(300.0);
                ui.spinner();
                ui.add_space(20.0);
                ui.colored_label(TEXT_PRIMARY, status);
            });
        });
    }

    fn show_name_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(200.0);
                
                ui.heading(egui::RichText::new("Paint App").size(48.0).color(TEXT_PRIMARY));
                ui.add_space(10.0);
                ui.colored_label(ACCENT_CYAN, "Real-time Collaborative Drawing");
                ui.add_space(40.0);
                
                ui.colored_label(TEXT_PRIMARY, "Enter your name to start");
                ui.add_space(10.0);
                
                let mut name = self.state.lock().unwrap().my_name_input.clone();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text("Your display name...")
                        .desired_width(300.0)
                        .font(egui::TextStyle::Heading)
                );
                self.state.lock().unwrap().my_name_input = name.clone();
                
                ui.add_space(10.0);
                
                ui.horizontal(|ui| {
                    ui.colored_label(TEXT_DIM, "Avatar color:");
                    let mut color = self.state.lock().unwrap().brush_color;
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        self.state.lock().unwrap().brush_color = color;
                    }
                });
                
                ui.add_space(20.0);
                
                let btn = ui.add(egui::Button::new(
                    egui::RichText::new("Start Drawing ->").size(20.0)
                ).fill(ACCENT_PURPLE).min_size(egui::vec2(200.0, 50.0)));
                
                if btn.clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    if !name.trim().is_empty() {
                        let color = self.state.lock().unwrap().brush_color;
                        let color_hex = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
                        self.call_reducer(|conn| {
                            let _ = conn.reducers.set_name(name.trim().to_string());
                            let _ = conn.reducers.set_avatar_color(color_hex);
                        });
                    }
                }
            });
        });
    }

    fn show_canvas_list(&mut self, ctx: &egui::Context, state: &AppState) {
        let my_user = self.get_my_user(state);
        let name = my_user.map(|u| u.name.as_str()).unwrap_or("User");

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.heading(egui::RichText::new("Select a Canvas").size(36.0).color(TEXT_PRIMARY));
                ui.add_space(10.0);
                ui.colored_label(ACCENT_CYAN, format!("Welcome, {}!", name));
                ui.add_space(30.0);
                
                if ui.add(egui::Button::new(
                    egui::RichText::new("+ Create New Canvas").size(18.0)
                ).fill(ACCENT_PURPLE).min_size(egui::vec2(250.0, 45.0))).clicked() {
                    self.call_reducer(|conn| {
                        let _ = conn.reducers.create_canvas("New Canvas".to_string());
                    });
                }
                
                ui.add_space(30.0);
                ui.separator();
                ui.add_space(20.0);
                
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for canvas in &state.canvases {
                            let users_on_canvas = state.users.iter()
                                .filter(|u| u.current_canvas_id == Some(canvas.id))
                                .count();
                            
                            // Check if I'm a member
                            let am_member = self.am_i_member_of(state, canvas.id);
                            
                            let frame = egui::Frame::none()
                                .fill(Color32::from_rgb(25, 25, 40))
                                .rounding(12.0)
                                .inner_margin(20.0);
                            
                            frame.show(ui, |ui| {
                                ui.set_min_size(egui::vec2(200.0, 140.0));
                                ui.vertical(|ui| {
                                    ui.colored_label(TEXT_PRIMARY, egui::RichText::new(&canvas.name).size(18.0).strong());
                                    ui.add_space(5.0);
                                    
                                    // User dots
                                    ui.horizontal(|ui| {
                                        for user in state.users.iter().filter(|u| u.current_canvas_id == Some(canvas.id)).take(5) {
                                            let color = parse_color(&user.avatar_color);
                                            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                            ui.painter().circle_filled(rect.center(), 6.0, color);
                                        }
                                        if users_on_canvas > 5 {
                                            ui.colored_label(TEXT_DIM, format!("+{}", users_on_canvas - 5));
                                        }
                                    });
                                    
                                    ui.add_space(5.0);
                                    ui.colored_label(TEXT_DIM, format!("{} users online", users_on_canvas));
                                    
                                    if am_member {
                                        ui.colored_label(Color32::GREEN, "(Member)");
                                    }
                                    
                                    ui.add_space(10.0);
                                    
                                    if ui.add(egui::Button::new("Join").fill(ACCENT_CYAN)).clicked() {
                                        let cid = canvas.id;
                                        self.call_reducer(|conn| {
                                            let _ = conn.reducers.join_canvas(cid);
                                        });
                                    }
                                });
                            });
                            ui.add_space(15.0);
                        }
                    });
                    
                    if state.canvases.is_empty() {
                        ui.add_space(50.0);
                        ui.colored_label(TEXT_DIM, "No canvases yet. Create one to get started!");
                    }
                });
            });
        });
    }

    fn show_paint_view(&mut self, ctx: &egui::Context, state: &AppState, canvas_id: u64) {
        let current_canvas = state.canvases.iter().find(|c| c.id == canvas_id);
        let canvas_name = current_canvas.map(|c| c.name.as_str()).unwrap_or("Canvas");
        let users_on_canvas: Vec<_> = state.users.iter()
            .filter(|u| u.current_canvas_id == Some(canvas_id))
            .collect();
        let canvas_strokes: Vec<_> = state.strokes.iter()
            .filter(|s| s.canvas_id == canvas_id)
            .collect();
        let canvas_elements: Vec<_> = state.elements.iter()
            .filter(|e| e.canvas_id == canvas_id)
            .collect();
        let canvas_layers: Vec<_> = state.layers.iter()
            .filter(|l| l.canvas_id == canvas_id)
            .collect();

        // Top bar
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("<- Back").clicked() {
                    self.call_reducer(|conn| {
                        let _ = conn.reducers.leave_canvas();
                    });
                }
                ui.separator();
                ui.colored_label(TEXT_PRIMARY, egui::RichText::new(canvas_name).size(18.0).strong());
                ui.separator();
                ui.colored_label(ACCENT_CYAN, format!("{} users", users_on_canvas.len()));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.selectable_label(state.show_chat_panel, "Chat").clicked() {
                        self.state.lock().unwrap().show_chat_panel = !state.show_chat_panel;
                    }
                    if ui.selectable_label(state.show_activity_panel, "Activity").clicked() {
                        self.state.lock().unwrap().show_activity_panel = !state.show_activity_panel;
                    }
                    if ui.selectable_label(state.show_layers_panel, "Layers").clicked() {
                        self.state.lock().unwrap().show_layers_panel = !state.show_layers_panel;
                    }
                    if ui.selectable_label(state.show_users_panel, "Users").clicked() {
                        self.state.lock().unwrap().show_users_panel = !state.show_users_panel;
                    }
                });
            });
        });

        // Toolbar
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tools = [
                    (Tool::Select, "Select", "V"),
                    (Tool::Brush, "Brush", "B"),
                    (Tool::Eraser, "Eraser", "E"),
                    (Tool::Rectangle, "Rect", "R"),
                    (Tool::Ellipse, "Ellipse", "O"),
                    (Tool::Line, "Line", "L"),
                    (Tool::Arrow, "Arrow", "A"),
                    (Tool::Text, "Text", "T"),
                ];
                
                for (tool, label, shortcut) in tools {
                    let is_selected = state.current_tool == tool;
                    let btn = ui.add(egui::Button::new(label)
                        .fill(if is_selected { ACCENT_PURPLE } else { Color32::from_rgb(40, 40, 60) }));
                    if btn.clicked() {
                        self.state.lock().unwrap().current_tool = tool.clone();
                        self.update_tool_on_server();
                    }
                    btn.on_hover_text(format!("Shortcut: {}", shortcut));
                }
                
                ui.separator();
                
                let mut stroke_color = state.brush_color;
                ui.colored_label(TEXT_DIM, "Stroke:");
                if ui.color_edit_button_srgba(&mut stroke_color).changed() {
                    self.state.lock().unwrap().brush_color = stroke_color;
                    let hex = format!("#{:02x}{:02x}{:02x}", stroke_color.r(), stroke_color.g(), stroke_color.b());
                    self.call_reducer(|conn| {
                        let _ = conn.reducers.set_selected_color(hex);
                    });
                }
                
                let mut fill_color = state.fill_color;
                ui.colored_label(TEXT_DIM, "Fill:");
                if ui.color_edit_button_srgba(&mut fill_color).changed() {
                    self.state.lock().unwrap().fill_color = fill_color;
                }
                
                ui.separator();
                
                let mut size = state.brush_size;
                ui.colored_label(TEXT_DIM, "Size:");
                if ui.add(egui::Slider::new(&mut size, 1.0..=50.0).show_value(false)).changed() {
                    self.state.lock().unwrap().brush_size = size;
                }
                ui.label(format!("{:.0}px", size));
            });
        });

        // Users panel
        if state.show_users_panel {
            egui::SidePanel::right("users_panel").default_width(180.0).show(ctx, |ui| {
                ui.heading(egui::RichText::new("Users").color(TEXT_PRIMARY));
                ui.separator();
                
                for user in &users_on_canvas {
                    let color = parse_color(&user.avatar_color);
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 6.0, color);
                        
                        ui.colored_label(TEXT_PRIMARY, &user.name);
                        
                        let status_color = match user.status.as_str() {
                            "active" => Color32::GREEN,
                            "idle" => Color32::YELLOW,
                            _ => Color32::GRAY,
                        };
                        let (rect2, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect2.center(), 4.0, status_color);
                    });
                    
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.colored_label(TEXT_DIM, format!("Tool: {}", user.current_tool));
                    });
                }
            });
        }

        // Layers panel
        if state.show_layers_panel {
            egui::SidePanel::left("layers_panel").default_width(200.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("Layers").color(TEXT_PRIMARY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("+").clicked() {
                            let num = canvas_layers.len() + 1;
                            self.call_reducer(|conn| {
                                let _ = conn.reducers.create_layer(canvas_id, format!("Layer {}", num));
                            });
                        }
                    });
                });
                ui.separator();
                
                let mut sorted_layers = canvas_layers.clone();
                sorted_layers.sort_by_key(|l| -l.order_index);
                
                for layer in sorted_layers {
                    let frame = egui::Frame::none()
                        .fill(Color32::from_rgb(30, 30, 45))
                        .rounding(6.0)
                        .inner_margin(8.0);
                    
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let vis_text = if layer.visible { "O" } else { "-" };
                            if ui.button(vis_text).clicked() {
                                let lid = layer.id;
                                self.call_reducer(|conn| {
                                    let _ = conn.reducers.toggle_layer_visibility(lid);
                                });
                            }
                            
                            ui.colored_label(TEXT_PRIMARY, &layer.name);
                            
                            if layer.locked_by.is_some() {
                                ui.colored_label(Color32::YELLOW, "[Locked]");
                            }
                        });
                        
                        let mut opacity = layer.opacity as f32;
                        if ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("Opacity")).changed() {
                            let lid = layer.id;
                            self.call_reducer(|conn| {
                                let _ = conn.reducers.set_layer_opacity(lid, opacity as f64);
                            });
                        }
                    });
                    ui.add_space(5.0);
                }
            });
        }

        // Chat panel
        if state.show_chat_panel {
            egui::SidePanel::right("chat_panel").default_width(280.0).show(ctx, |ui| {
                ui.heading(egui::RichText::new("Chat").color(TEXT_PRIMARY));
                ui.separator();
                
                let canvas_messages: Vec<_> = state.chat_messages.iter()
                    .filter(|m| m.canvas_id == canvas_id)
                    .collect();
                
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    for msg in &canvas_messages {
                        let sender = state.users.iter().find(|u| u.identity == msg.sender);
                        let sender_name = sender.map(|u| u.name.as_str()).unwrap_or("Unknown");
                        let sender_color = sender.map(|u| parse_color(&u.avatar_color)).unwrap_or(ACCENT_CYAN);
                        
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, sender_color);
                            ui.colored_label(sender_color, sender_name);
                        });
                        ui.colored_label(TEXT_PRIMARY, &msg.text);
                        ui.add_space(8.0);
                    }
                });
                
                ui.separator();
                
                let mut input = state.chat_input.clone();
                let response = ui.add(egui::TextEdit::singleline(&mut input)
                    .hint_text("Type a message...")
                    .desired_width(ui.available_width() - 60.0));
                self.state.lock().unwrap().chat_input = input.clone();
                
                if response.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !input.is_empty() {
                    self.call_reducer(|conn| {
                        let _ = conn.reducers.send_chat_message(canvas_id, input.clone());
                    });
                    self.state.lock().unwrap().chat_input.clear();
                }
            });
        }

        // Activity panel
        if state.show_activity_panel {
            egui::SidePanel::left("activity_panel").default_width(250.0).show(ctx, |ui| {
                ui.heading(egui::RichText::new("Activity").color(TEXT_PRIMARY));
                ui.separator();
                
                let canvas_activity: Vec<_> = state.activity_entries.iter()
                    .filter(|a| a.canvas_id == canvas_id)
                    .collect();
                
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for entry in canvas_activity.iter().rev().take(50) {
                        let user = state.users.iter().find(|u| u.identity == entry.user_identity);
                        let color = user.map(|u| parse_color(&u.avatar_color)).unwrap_or(TEXT_DIM);
                        
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, color);
                            ui.colored_label(TEXT_PRIMARY, &entry.description);
                        });
                        ui.add_space(4.0);
                    }
                });
            });
        }

        // Main canvas
        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) = ui.allocate_painter(
                ui.available_size(),
                egui::Sense::click_and_drag(),
            );
            
            let rect = response.rect;
            
            // Background
            painter.rect_filled(rect, 0.0, BG_COLOR);
            
            // Grid
            let grid_size = 20.0;
            let grid_color = Color32::from_rgba_unmultiplied(255, 255, 255, 5);
            let mut x = rect.min.x;
            while x < rect.max.x {
                painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], Stroke::new(1.0, grid_color));
                x += grid_size;
            }
            let mut y = rect.min.y;
            while y < rect.max.y {
                painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], Stroke::new(1.0, grid_color));
                y += grid_size;
            }

            // Draw strokes
            for stroke in &canvas_strokes {
                if let Ok(points) = serde_json::from_str::<Vec<Point>>(&stroke.points_json) {
                    if points.len() >= 2 {
                        let color = if stroke.tool == "eraser" {
                            BG_COLOR
                        } else {
                            parse_color(&stroke.color)
                        };
                        let egui_points: Vec<Pos2> = points.iter()
                            .map(|p| Pos2::new(rect.min.x + p.x as f32, rect.min.y + p.y as f32))
                            .collect();
                        
                        painter.add(egui::Shape::line(egui_points, Stroke::new(stroke.size as f32, color)));
                    }
                }
            }

            // Draw elements
            for element in &canvas_elements {
                let elem_rect = Rect::from_min_size(
                    Pos2::new(rect.min.x + element.x as f32, rect.min.y + element.y as f32),
                    Vec2::new(element.width as f32, element.height as f32),
                );
                let stroke_color = parse_color(&element.stroke_color);
                let fill_color = parse_color(&element.fill_color);
                
                match element.element_type.as_str() {
                    "rectangle" => {
                        if fill_color.a() > 0 {
                            painter.rect_filled(elem_rect, 0.0, fill_color);
                        }
                        painter.rect_stroke(elem_rect, 0.0, Stroke::new(element.stroke_width as f32, stroke_color));
                    }
                    "ellipse" => {
                        let center = elem_rect.center();
                        let radius = Vec2::new(elem_rect.width() / 2.0, elem_rect.height() / 2.0);
                        if fill_color.a() > 0 {
                            painter.add(egui::Shape::ellipse_filled(center, radius, fill_color));
                        }
                        painter.add(egui::Shape::ellipse_stroke(center, radius, Stroke::new(element.stroke_width as f32, stroke_color)));
                    }
                    "line" | "arrow" => {
                        let start = elem_rect.min;
                        let end = Pos2::new(elem_rect.max.x, elem_rect.max.y);
                        painter.line_segment([start, end], Stroke::new(element.stroke_width as f32, stroke_color));
                        
                        if element.element_type == "arrow" {
                            let dir = (end - start).normalized();
                            let perp = Vec2::new(-dir.y, dir.x);
                            let arrow_size = 15.0;
                            let p1 = end - dir * arrow_size + perp * arrow_size * 0.4;
                            let p2 = end - dir * arrow_size - perp * arrow_size * 0.4;
                            painter.add(egui::Shape::convex_polygon(vec![end, p1, p2], stroke_color, Stroke::NONE));
                        }
                    }
                    "text" | "sticky" => {
                        if element.element_type == "sticky" {
                            painter.rect_filled(elem_rect, 4.0, Color32::from_rgb(255, 255, 150));
                        }
                        if let Some(text) = &element.text_content {
                            painter.text(
                                elem_rect.min + Vec2::new(5.0, 5.0),
                                egui::Align2::LEFT_TOP,
                                text,
                                egui::FontId::proportional(14.0),
                                if element.element_type == "sticky" { Color32::BLACK } else { stroke_color },
                            );
                        }
                    }
                    _ => {}
                }
                
                // Selection highlight
                if state.selected_element_ids.contains(&element.id) {
                    painter.rect_stroke(elem_rect.expand(2.0), 0.0, Stroke::new(2.0, ACCENT_CYAN));
                    
                    let handle_size = 8.0;
                    let handles = [
                        elem_rect.left_top(), elem_rect.center_top(), elem_rect.right_top(),
                        elem_rect.left_center(), elem_rect.right_center(),
                        elem_rect.left_bottom(), elem_rect.center_bottom(), elem_rect.right_bottom(),
                    ];
                    for handle in handles {
                        painter.rect_filled(
                            Rect::from_center_size(handle, Vec2::splat(handle_size)),
                            2.0,
                            ACCENT_CYAN,
                        );
                    }
                }
            }

            // Current stroke preview
            {
                let current_stroke = self.state.lock().unwrap().current_stroke_points.clone();
                if current_stroke.len() >= 2 {
                    let color = if state.current_tool == Tool::Eraser {
                        BG_COLOR
                    } else {
                        state.brush_color
                    };
                    painter.add(egui::Shape::line(current_stroke, Stroke::new(state.brush_size, color)));
                }
            }

            // Shape preview
            if let Some((start, end)) = state.shape_preview {
                let preview_rect = Rect::from_two_pos(start, end);
                let stroke = Stroke::new(state.brush_size, state.brush_color);
                
                match &state.current_tool {
                    Tool::Rectangle => {
                        if state.fill_color.a() > 0 {
                            painter.rect_filled(preview_rect, 0.0, state.fill_color);
                        }
                        painter.rect_stroke(preview_rect, 0.0, stroke);
                    }
                    Tool::Ellipse => {
                        let center = preview_rect.center();
                        let radius = Vec2::new(preview_rect.width() / 2.0, preview_rect.height() / 2.0);
                        if state.fill_color.a() > 0 {
                            painter.add(egui::Shape::ellipse_filled(center, radius, state.fill_color));
                        }
                        painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
                    }
                    Tool::Line | Tool::Arrow => {
                        painter.line_segment([start, end], stroke);
                        if state.current_tool == Tool::Arrow {
                            let dir = (end - start).normalized();
                            let perp = Vec2::new(-dir.y, dir.x);
                            let arrow_size = 15.0;
                            let p1 = end - dir * arrow_size + perp * arrow_size * 0.4;
                            let p2 = end - dir * arrow_size - perp * arrow_size * 0.4;
                            painter.add(egui::Shape::convex_polygon(vec![end, p1, p2], state.brush_color, Stroke::NONE));
                        }
                    }
                    _ => {}
                }
            }

            // Other users' cursors
            for user in &users_on_canvas {
                if state.my_identity.as_ref().map_or(false, |id| user.identity == *id) {
                    continue;
                }
                if user.cursor_x == 0.0 && user.cursor_y == 0.0 {
                    continue;
                }
                
                let pos = Pos2::new(rect.min.x + user.cursor_x as f32, rect.min.y + user.cursor_y as f32);
                let color = parse_color(&user.avatar_color);
                
                painter.circle_filled(pos, 10.0, color);
                painter.text(pos + Vec2::new(15.0, 0.0), egui::Align2::LEFT_CENTER, &user.current_tool, egui::FontId::default(), color);
                painter.text(pos + Vec2::new(15.0, 15.0), egui::Align2::LEFT_TOP, &user.name, egui::FontId::proportional(12.0), TEXT_PRIMARY);
                
                let user_color = parse_color(&user.selected_color);
                painter.circle_filled(pos + Vec2::new(-8.0, 8.0), 5.0, user_color);
            }

            // Input handling
            let current_tool = state.current_tool.clone();
            
            if let Some(pos) = response.hover_pos() {
                let x = (pos.x - rect.min.x) as f64;
                let y = (pos.y - rect.min.y) as f64;
                self.call_reducer(|conn| {
                    let _ = conn.reducers.update_cursor(x, y);
                });
            }
            
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    match &current_tool {
                        Tool::Brush | Tool::Eraser => {
                            self.state.lock().unwrap().current_stroke_points = vec![pos];
                            self.state.lock().unwrap().is_drawing = true;
                        }
                        Tool::Rectangle | Tool::Ellipse | Tool::Line | Tool::Arrow => {
                            self.state.lock().unwrap().drag_start = Some(pos);
                        }
                        Tool::Select => {
                            let mut found = None;
                            for element in canvas_elements.iter().rev() {
                                let elem_rect = Rect::from_min_size(
                                    Pos2::new(rect.min.x + element.x as f32, rect.min.y + element.y as f32),
                                    Vec2::new(element.width as f32, element.height as f32),
                                );
                                if elem_rect.contains(pos) {
                                    found = Some(element.id);
                                    break;
                                }
                            }
                            
                            if let Some(id) = found {
                                let mut s = self.state.lock().unwrap();
                                if ctx.input(|i| i.modifiers.shift) {
                                    if !s.selected_element_ids.contains(&id) {
                                        s.selected_element_ids.push(id);
                                    }
                                } else {
                                    s.selected_element_ids = vec![id];
                                }
                                let ids = s.selected_element_ids.clone();
                                drop(s);
                                let ids_json = serde_json::to_string(&ids).unwrap_or_default();
                                self.call_reducer(|conn| {
                                    let _ = conn.reducers.select_elements(canvas_id, ids_json);
                                });
                            } else {
                                self.deselect_all();
                            }
                        }
                        _ => {}
                    }
                }
            }
            
            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    match &current_tool {
                        Tool::Brush | Tool::Eraser => {
                            self.state.lock().unwrap().current_stroke_points.push(pos);
                        }
                        Tool::Rectangle | Tool::Ellipse | Tool::Line | Tool::Arrow => {
                            let start = self.state.lock().unwrap().drag_start;
                            if let Some(start) = start {
                                let end = if ctx.input(|i| i.modifiers.shift) {
                                    let dx = (pos.x - start.x).abs();
                                    let dy = (pos.y - start.y).abs();
                                    let size = dx.max(dy);
                                    Pos2::new(
                                        start.x + size * (pos.x - start.x).signum(),
                                        start.y + size * (pos.y - start.y).signum(),
                                    )
                                } else {
                                    pos
                                };
                                self.state.lock().unwrap().shape_preview = Some((start, end));
                            }
                        }
                        _ => {}
                    }
                }
            }
            
            if response.drag_stopped() {
                let current_tool = self.state.lock().unwrap().current_tool.clone();
                
                match current_tool {
                    Tool::Brush | Tool::Eraser => {
                        let points = self.state.lock().unwrap().current_stroke_points.clone();
                        if points.len() > 1 {
                            let layer = canvas_layers.first();
                            if let Some(layer) = layer {
                                let points_data: Vec<Point> = points.iter()
                                    .map(|p| Point { x: (p.x - rect.min.x) as f64, y: (p.y - rect.min.y) as f64 })
                                    .collect();
                                let points_json = serde_json::to_string(&points_data).unwrap_or_default();
                                let color = if current_tool == Tool::Eraser {
                                    "#0a0a0f".to_string()
                                } else {
                                    let c = self.state.lock().unwrap().brush_color;
                                    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
                                };
                                let size = self.state.lock().unwrap().brush_size as f64;
                                let tool_str = current_tool.as_str().to_string();
                                let lid = layer.id;
                                
                                self.call_reducer(|conn| {
                                    let _ = conn.reducers.add_stroke(canvas_id, lid, points_json, color, size, tool_str);
                                });
                            }
                        }
                        self.state.lock().unwrap().current_stroke_points.clear();
                        self.state.lock().unwrap().is_drawing = false;
                    }
                    Tool::Rectangle | Tool::Ellipse | Tool::Line | Tool::Arrow => {
                        let preview = self.state.lock().unwrap().shape_preview;
                        if let Some((start, end)) = preview {
                            let layer = canvas_layers.first();
                            if let Some(layer) = layer {
                                let x = (start.x.min(end.x) - rect.min.x) as f64;
                                let y = (start.y.min(end.y) - rect.min.y) as f64;
                                let width = (end.x - start.x).abs() as f64;
                                let height = (end.y - start.y).abs() as f64;
                                
                                let stroke_color = {
                                    let c = self.state.lock().unwrap().brush_color;
                                    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
                                };
                                let fill_color = {
                                    let c = self.state.lock().unwrap().fill_color;
                                    if c.a() > 0 {
                                        format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
                                    } else {
                                        "transparent".to_string()
                                    }
                                };
                                let stroke_width = self.state.lock().unwrap().brush_size as f64;
                                let element_type = current_tool.as_str().to_string();
                                let lid = layer.id;
                                
                                self.call_reducer(|conn| {
                                    let _ = conn.reducers.add_element(
                                        canvas_id, lid, element_type,
                                        x, y, width, height,
                                        stroke_color, fill_color, stroke_width,
                                        None, "medium".to_string(), None
                                    );
                                });
                            }
                        }
                        self.state.lock().unwrap().shape_preview = None;
                        self.state.lock().unwrap().drag_start = None;
                    }
                    _ => {}
                }
            }
        });

        // Status bar
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let other_cursors = users_on_canvas.iter()
                    .filter(|u| state.my_identity.as_ref().map_or(true, |id| u.identity != *id))
                    .count();
                ui.colored_label(TEXT_DIM, format!("Live cursors: {}", other_cursors));
                ui.separator();
                ui.colored_label(TEXT_DIM, format!("Tool: {}", state.current_tool.as_str()));
                ui.separator();
                ui.colored_label(TEXT_DIM, "V/B/E/R/O/L/A/T = tools | Del = delete | Esc = deselect");
            });
        });
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Point {
    x: f64,
    y: f64,
}

fn parse_color(color_str: &str) -> Color32 {
    if color_str == "transparent" {
        return Color32::TRANSPARENT;
    }
    if color_str.starts_with('#') && color_str.len() == 7 {
        let r = u8::from_str_radix(&color_str[1..3], 16).unwrap_or(100);
        let g = u8::from_str_radix(&color_str[3..5], 16).unwrap_or(100);
        let b = u8::from_str_radix(&color_str[5..7], 16).unwrap_or(241);
        Color32::from_rgb(r, g, b)
    } else {
        ACCENT_PURPLE
    }
}
