//! The start menu

use super::pause_menu::{Metrics, inside, out};
use super::*;

mod bar;
mod join;
mod pages;
mod screen;

pub const START_ENTRIES: [&str; 5] = ["start", "continue", "join", "options", "quit"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartPage {
    #[default]
    Main,
    Maps,
    Join,
    Options,
}

/// A line of the vehicle list: a manufacturer (`vehicle` is None) or one of its models.
#[derive(Clone, Debug, Default)]
pub struct VehRow {
    pub label: String,
    /// Header: how many models the manufacturer has.
    pub count: usize,
    /// Header: the models are shown.
    pub open: bool,
    pub vehicle: Option<usize>,
    pub on: bool,
}

/// A line of the timetable in the list: its number and where it goes.
#[derive(Clone, Debug, Default)]
pub struct LineRow {
    pub badge: String,
    pub label: String,
}

/// A tour of the chosen line: its name and when it drives.
#[derive(Clone, Debug, Default)]
pub struct TourRow {
    pub label: String,
    pub span: String,
}

pub struct StartView<'a> {
    pub page: StartPage,
    pub sel: usize,
    pub can_continue: bool,
    pub maps: &'a [String],
    pub map: usize,
    pub map_top: usize,
    pub rows: &'a [(String, String)],
    pub row: usize,
    pub zone: usize,
    pub address: &'a str,
    pub caret: bool,
    /// The chosen vehicle's maker, its place in the list and how many there are.
    pub maker: &'a str,
    pub vehicle: usize,
    pub vehicles: usize,
    pub vehicle_names: &'a [String],
    pub veh_rows: &'a [VehRow],
    pub veh_top: usize,
    pub query: &'a str,
    /// The dropdown open (8 livery, 9 depot file, 10 fleet number; 0 none), its choices,
    /// the chosen one and the scroll.
    pub drop: usize,
    pub drop_items: &'a [String],
    pub drop_sel: usize,
    pub drop_top: usize,
    /// 0 = nothing typed into, 1 = the search, 2 = the number plate.
    pub edit: u8,
    pub details: bool,
    pub livery: &'a str,
    pub depot: &'a str,
    pub fleet: &'a str,
    pub plate: &'a str,
    /// 0 = the overview, 1 to 4 = the tab of the editor.
    pub sect: usize,
    /// The line of the overview in focus.
    pub focus: usize,
    /// 0 = by date, 1 to 4 = spring, summer, autumn, winter.
    pub season: usize,
    /// The time typed (HH:MM), the calendar under the date: open, its title and its 42 days
    /// (number, in the month shown, the chosen one).
    pub time_text: &'a str,
    pub cal_open: bool,
    pub cal_title: &'a str,
    pub cal_cells: &'a [(i32, bool, bool)],
    /// The weather choices (icon, label), the chosen one, the scroll and the settings of
    /// the custom weather (label, value).
    pub weathers: &'a [(String, String)],
    pub weather: usize,
    pub weather_top: usize,
    pub custom: &'a [(String, String)],
    /// No tour chosen: drive freely instead of starting on a route.
    pub free: bool,
    /// The lines and tours the filters let through, the chosen one among them, the scroll,
    /// the filters' text, and the sentence under the title of the map page.
    pub lines: &'a [LineRow],
    pub line_sel: Option<usize>,
    pub line_top: usize,
    pub line_query: &'a str,
    pub tours: &'a [TourRow],
    pub tour_sel: Option<usize>,
    pub tour_top: usize,
    pub tour_query: &'a str,
    pub route_sub: &'a str,
    /// The start points of the map (free drive), the chosen one and the scroll.
    pub entries: &'a [String],
    /// The start point as text ("automatic" when none is chosen).
    pub entry_text: &'a str,
    pub entry_sel: Option<usize>,
    pub entry_top: usize,
}

#[derive(Default)]
pub struct StartUi {
    pub active: bool,
    pub hand: bool,
    pub items: Vec<[f32; 4]>,
    pub tabs: Vec<[f32; 4]>,
    pub maps: Vec<[f32; 4]>,
    pub map_vis: usize,
    pub rows: Vec<[f32; 4]>,
    pub arrows: Vec<[f32; 4]>,
    /// Free drive and route buttons.
    pub mode: Vec<[f32; 4]>,
    /// The lines of the overview, the tabs and the close button of the editor.
    pub summary: Vec<[f32; 4]>,
    pub sect_tabs: Vec<[f32; 4]>,
    pub close: [f32; 4],
    /// The quit button on the left of the bar.
    pub quit: [f32; 4],
    pub next: [f32; 4],
    pub cont: [f32; 4],
    pub veh: Vec<[f32; 4]>,
    pub veh_vis: usize,
    pub search: [f32; 4],
    pub plate: [f32; 4],
    /// The fields of the dropdowns, the lines of the open one, its frame and its first line.
    pub drops: Vec<[f32; 4]>,
    pub drop_items: Vec<[f32; 4]>,
    pub drop_box: [f32; 4],
    pub drop_first: usize,
    pub details: [f32; 4],
    /// The size of the picture of the bus, and the texture the game drew it into.
    pub pic_size: (u32, u32),
    pub picture: Option<::render::TextureId>,
    pub status: String,
    pub seasons: Vec<[f32; 4]>,
    pub time_field: [f32; 4],
    pub date_field: [f32; 4],
    pub now: [f32; 4],
    /// The calendar: its 42 days and its buttons (previous, next, close, today).
    pub cal_days: Vec<[f32; 4]>,
    pub cal_btn: Vec<[f32; 4]>,
    pub weathers: Vec<[f32; 4]>,
    pub weather_vis: usize,
    pub go: [f32; 4],
    pub field: [f32; 4],
    /// The rows of the line and tour lists, how many fit, their frames and filter fields.
    pub lines: Vec<[f32; 4]>,
    pub tours: Vec<[f32; 4]>,
    pub line_vis: usize,
    pub tour_vis: usize,
    pub line_box: [f32; 4],
    pub tour_box: [f32; 4],
    /// The start points' rows, how many fit and their frame.
    pub entries: Vec<[f32; 4]>,
    pub entry_vis: usize,
    pub entry_box: [f32; 4],
    pub line_filter: [f32; 4],
    pub tour_filter: [f32; 4],
    /// The picture of the map and route (the game draws it), the size wanted, what to say
    /// when it is not there, and the stops to name (place in the picture, 0 to 1).
    pub route_pic: Option<::render::TextureId>,
    pub route_size: (u32, u32),
    pub route_rect: [f32; 4],
    pub route_status: String,
    pub stops: Vec<(String, f32, f32)>,
    t: f32,
    page_t: f32,
    sect_t: f32,
    enter: f32,
    page_e: f32,
    sect_e: f32,
    prev_sect: usize,
    last_sect: Option<usize>,
    last: Option<StartPage>,
}

impl StartUi {
    fn clear(&mut self) {
        self.items.clear();
        self.tabs.clear();
        self.quit = [0.0; 4];
        self.maps.clear();
        self.rows.clear();
        self.arrows.clear();
        self.mode.clear();
        self.summary.clear();
        self.sect_tabs.clear();
        self.veh.clear();
        self.search = [0.0; 4];
        self.plate = [0.0; 4];
        self.drops.clear();
        self.drop_items.clear();
        self.drop_box = [0.0; 4];
        self.details = [0.0; 4];
        self.seasons.clear();
        self.close = [0.0; 4];
        self.next = [0.0; 4];
        self.cont = [0.0; 4];
        self.time_field = [0.0; 4];
        self.date_field = [0.0; 4];
        self.now = [0.0; 4];
        self.cal_days.clear();
        self.cal_btn.clear();
        self.weathers.clear();
        self.go = [0.0; 4];
        self.field = [0.0; 4];
        self.lines.clear();
        self.tours.clear();
        self.line_box = [0.0; 4];
        self.tour_box = [0.0; 4];
        self.entries.clear();
        self.entry_box = [0.0; 4];
        self.line_filter = [0.0; 4];
        self.tour_filter = [0.0; 4];
        self.route_rect = [0.0; 4];
    }
}

impl Ui {
    pub fn start_reset(&mut self) {
        self.start = StartUi::default();
        self.pause_reset();
    }

    pub fn draw_start(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, dt: f32, v: &StartView) {
        self.text.backdrop = f.opacity;
        self.anim_dt = dt.clamp(0.0, 0.1);
        self.text.flat = true;
        self.start.active = true;
        let m = Metrics::new(f);
        if self.start.last != Some(v.page) {
            self.start.last = Some(v.page);
            self.start.page_t = 0.0;
            self.pause_reset();
        }
        self.start.page_t = (self.start.page_t + self.anim_dt / 0.3).min(1.0);
        self.start.t = (self.start.t + self.anim_dt / 0.35).min(1.0);
        self.start.clear();
        let at = |rs: &[[f32; 4]]| rs.iter().any(|rc| inside(*rc, f.cursor));
        match v.page {
            StartPage::Main => {
                self.draw_start_screen(r, scene, m, v);
                let items: Vec<[f32; 4]> = self
                    .start
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != 1 || v.can_continue)
                    .map(|(_, rc)| *rc)
                    .collect();
                self.hand = at(&items);
            }
            StartPage::Maps => {
                self.draw_start_maps(r, scene, f, m, v);
                self.hand = at(&self.start.items)
                    || at(&self.start.maps)
                    || at(&self.start.rows)
                    || at(&self.start.arrows)
                    || at(&self.start.mode)
                    || at(&self.start.summary)
                    || at(&self.start.sect_tabs)
                    || at(&self.start.veh)
                    || at(&[self.start.search, self.start.plate, self.start.details])
                    || at(&self.start.drops)
                    || at(&self.start.drop_items)
                    || at(&self.start.lines)
                    || at(&self.start.tours)
                    || at(&self.start.entries)
                    || at(&[self.start.line_filter, self.start.tour_filter])
                    || at(&self.start.seasons)
                    || at(&[self.start.close, self.start.next, self.start.cont])
                    || at(&[self.start.time_field, self.start.date_field, self.start.now])
                    || at(&self.start.cal_days)
                    || at(&self.start.cal_btn)
                    || at(&self.start.weathers)
                    || at(&self.start.tabs) || at(&[self.start.go, self.start.quit]);
            }
            StartPage::Join => {
                self.draw_start_join(r, scene, f, m, v);
                self.hand = at(&self.start.tabs) || at(&[self.start.go, self.start.field, self.start.quit]);
            }
            StartPage::Options => {
                self.text.rounded(r, scene, [0.0, 0.0, m.w, m.h], 0.0, [10, 12, 16, 255]);
                self.draw_pause(r, scene, f, PauseState { page: Some(OPTIONS_PAGE), sel: 0 });
                self.hand |= at(&self.start.tabs) || at(&[self.start.quit]);
            }
        }
        self.start.hand = self.hand;
        self.text.flat = false;
        self.text.end_frame(r, scene);
    }
}

pub(super) fn head_alpha(pt: f32) -> f32 {
    out(pt / 0.6)
}

pub(super) const BG: [u8; 4] = [10, 12, 16, 255];
/// The accent for text: alpha 0 is plain text (255 would draw it with an outline that glows).
pub(super) const ACCENT_TEXT: [u8; 4] = [232, 160, 48, 0];
