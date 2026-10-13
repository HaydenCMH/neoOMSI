use super::*;

impl Ui {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_tab_time(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, a: [f32; 4], v: &StartView, u: f32, hits: &mut Hits) {
        let g = 10.0 * u;
        let cap = (16.0 * u).round();
        let fh = (52.0 * u).round();
        let w = a[2] - a[0];
        let tw = ((w - g) * 0.42).floor();
        let mut y = a[1];
        self.start_caption(r, scene, "start.maps.time", a[0] + 4.0 * u, y, u);
        self.start_caption(r, scene, "start.maps.date", a[0] + tw + g + 4.0 * u, y, u);

        let nl = self.text.label(r, scene, "start.maps.now", (14.0 * u) as u32, ACCENT_TEXT);
        let ni = (16.0 * u).round();
        let total = nl.w as f32 + ni + 6.0 * u;
        let nrc = [a[2] - total - 10.0 * u, y - 6.0 * u, a[2] + 4.0 * u, y + cap + 4.0 * u];
        let nv = self.ease((229, "now", 0), if inside(nrc, f.cursor) { 1.0 } else { 0.0 }, 10.0);
        if nv > 0.0 {
            self.text.rounded(r, scene, nrc, 6.0 * u, fade([255, 255, 255, 12], nv));
        }
        let nx = a[2] - total;
        let ny = y + (cap - nl.h as f32) * 0.5;
        if let Some(ic) = self.text.icon(r, scene, "schedule", ni as u32, ACCENT) {
            ic.place(scene, nx, ny + (nl.h as f32 - ni) * 0.5);
        }
        nl.place(scene, nx + ni + 6.0 * u, ny);
        self.start.now = nrc;

        y += cap + 6.0 * u;
        let trc = [a[0], y, a[0] + tw, y + fh];
        let drc = [a[0] + tw + g, y, a[2], y + fh];
        self.start_field(r, scene, f, trc, None, Some("schedule"), v.time_text, "00:00", v.edit == 5, v.caret, 4, u);
        let date = v.rows.get(6).map(|x| x.1.as_str()).unwrap_or("-");
        self.start_field(r, scene, f, drc, None, Some("calendar_month"), date, "", v.cal_open, false, 5, u);
        self.start.time_field = trc;
        self.start.date_field = drc;
        y += fh + g;

        if v.cal_open {
            self.start_calendar(r, scene, f, [a[0], y, a[2], a[3]], v, u);
            return;
        }

        self.start_caption(r, scene, "start.maps.season", a[0] + 4.0 * u, y, u);
        y += cap + 6.0 * u;
        let sh = (44.0 * u).round();
        let seg = [a[0], y, a[2], y + sh];
        self.text.rounded(r, scene, seg, sh * 0.5, OPT_GROUP);
        let inset = 4.0 * u;
        let n = SEASONS.len();
        let bw = ((w - inset * 2.0 - 2.0 * u * (n - 1) as f32) / n as f32).floor();
        for (k, key) in SEASONS.iter().enumerate() {
            let x = a[0] + inset + k as f32 * (bw + 2.0 * u);
            let b = [x, seg[1] + inset, x + bw, seg[3] - inset];
            let on = v.season == k;
            let hot = inside(b, f.cursor);
            let ov = self.ease((223, "season", k), if on { 1.0 } else { 0.0 }, 12.0);
            let hv = self.ease((223, "seasonhot", k), if hot && !on { 1.0 } else { 0.0 }, 12.0);
            self.text.rounded(r, scene, b, (b[3] - b[1]) * 0.5, mix(fade(LIT, hv), ACCENT_SOFT, ov));
            let l = self.text.label(r, scene, key, (14.0 * u) as u32, if on { WHITE } else { mix(SOFT, WHITE, hv) });
            l.place(scene, b[0] + (bw - l.w as f32) * 0.5, (b[1] + b[3] - l.h as f32) * 0.5);
            self.start.seasons.push(b);
        }
        y += sh + g;
        self.start_tab_weather(r, scene, f, [a[0], y, a[2], a[3]], v, u, hits);
    }

    pub(super) fn start_calendar(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, a: [f32; 4], v: &StartView, u: f32) {
        let pad = (10.0 * u).round();
        let head_h = (42.0 * u).round();
        let wk_h = (24.0 * u).round();
        let cell_h = (34.0 * u).round();
        let foot_h = (40.0 * u).round();
        let h = pad * 2.0 + head_h + wk_h + cell_h * 6.0 + foot_h;
        let bx = [a[0], a[1], a[2], (a[1] + h).min(a[3].max(a[1] + h))];
        self.text.rounded(r, scene, bx, 12.0 * u, OPT_ROW);
        let (x0, x1) = (bx[0] + pad, bx[2] - pad);
        let mut y = bx[1] + pad;

        let tl = self.text.label(r, scene, v.cal_title, (16.0 * u) as u32, WHITE);
        tl.place(scene, x0 + 6.0 * u, y + (head_h - tl.h as f32) * 0.5);
        let bs = (34.0 * u).round();
        let isz = (22.0 * u).round();
        let next = [x1 - bs, y + (head_h - bs) * 0.5, x1, y + (head_h + bs) * 0.5];
        let prev = [next[0] - bs - 4.0 * u, next[1], next[0] - 4.0 * u, next[3]];
        let mut btn = vec![prev, next];
        for (b, icon) in [(prev, "expand_less"), (next, "expand_more")] {
            let hv = self.ease((230, icon, 0), if inside(b, f.cursor) { 1.0 } else { 0.0 }, 12.0);
            self.text.rounded(r, scene, b, bs * 0.5, fade(LIT, hv));
            if let Some(ic) = self.text.icon(r, scene, icon, isz as u32, mix(MUTED, WHITE, hv)) {
                ic.place(scene, b[0] + (bs - isz) * 0.5, b[1] + (bs - isz) * 0.5);
            }
        }
        y += head_h;

        let cw = ((x1 - x0) / 7.0).floor();
        let gx = x0 + ((x1 - x0) - cw * 7.0) * 0.5;
        for d in 0..7 {
            let key = format!("start.cal.d{}", d + 1);
            let l = self.text.label(r, scene, &key, (12.0 * u) as u32, MUTED);
            l.place(scene, gx + d as f32 * cw + (cw - l.w as f32) * 0.5, y + (wk_h - l.h as f32) * 0.5);
        }
        y += wk_h;

        for (k, (day, inside_month, chosen)) in v.cal_cells.iter().enumerate() {
            let (col, row) = (k % 7, k / 7);
            let rc = [gx + col as f32 * cw, y + row as f32 * cell_h, gx + (col + 1) as f32 * cw, y + (row + 1) as f32 * cell_h];
            let hot = inside(rc, f.cursor);
            let hv = self.ease((228, "cal", k), if hot && !*chosen { 1.0 } else { 0.0 }, 14.0);
            let cv = self.ease((228, "calon", k), if *chosen { 1.0 } else { 0.0 }, 14.0);
            let ir = [rc[0] + 2.0 * u, rc[1] + 2.0 * u, rc[2] - 2.0 * u, rc[3] - 2.0 * u];
            if hv > 0.0 {
                self.text.rounded(r, scene, ir, 8.0 * u, fade(LIT, hv));
            }
            if cv > 0.0 {
                self.text.rounded(r, scene, ir, 8.0 * u, fade(ACCENT_SOFT, cv));
            }
            let base = if *inside_month { SOFT } else { MUTED };
            let colr = if *chosen { WHITE } else { mix(base, WHITE, hv) };
            let l = self.text.label(r, scene, &day.to_string(), (14.0 * u) as u32, colr);
            l.place(scene, rc[0] + (cw - l.w as f32) * 0.5, rc[1] + (cell_h - l.h as f32) * 0.5);
            self.start.cal_days.push(rc);
        }
        y += cell_h * 6.0;

        let px = (14.0 * u) as u32;
        let cl = self.text.label(r, scene, "start.maps.close", px, ACCENT_TEXT);
        let tdl = self.text.label(r, scene, "start.maps.today", px, ACCENT_TEXT);
        let crc = [x0, y + 4.0 * u, x0 + cl.w as f32 + 20.0 * u, y + foot_h - 4.0 * u];
        let trc = [x1 - tdl.w as f32 - 20.0 * u, y + 4.0 * u, x1, y + foot_h - 4.0 * u];
        for (k, (rc, l)) in [(crc, cl), (trc, tdl)].into_iter().enumerate() {
            let hv = self.ease((230, "foot", k), if inside(rc, f.cursor) { 1.0 } else { 0.0 }, 12.0);
            self.text.rounded(r, scene, rc, 6.0 * u, fade(LIT, hv));
            l.place(scene, rc[0] + (rc[2] - rc[0] - l.w as f32) * 0.5, rc[1] + (rc[3] - rc[1] - l.h as f32) * 0.5);
            btn.push(rc);
        }
        self.start.cal_btn = btn;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_tab_weather(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, a: [f32; 4], v: &StartView, u: f32, hits: &mut Hits) {
        let custom = v.weather == 3;
        let th = (46.0 * u).round();
        let g = 4.0 * u;
        let cap = (16.0 * u).round();
        self.start_caption(r, scene, "start.maps.weather", a[0] + 4.0 * u, a[1], u);
        let a = [a[0], a[1] + cap + 6.0 * u, a[2], a[3]];
        let ctl = if custom { (th + g) * v.custom.len() as f32 + 8.0 * u } else { 0.0 };
        let list = [a[0], a[1], a[2], (a[3] - ctl).max(a[1] + ROW_H * u * 3.0)];
        let mut rects = Vec::new();
        self.start.weather_vis = self.start_weather_list(r, scene, f, list, v, u, &mut rects);
        self.start.weathers = rects;
        if custom {
            let mut y = list[3] + 8.0 * u;
            for (k, (label, value)) in v.custom.iter().enumerate() {
                self.start_stepper(r, scene, f, [a[0], y, a[2], y + th], 12 + k, label, value, v, u, false, None, hits);
                y += th + g;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_weather_list(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, a: [f32; 4], v: &StartView, u: f32, rects: &mut Vec<[f32; 4]>) -> usize {
        let row_h = (ROW_H * u).round();
        let vis = (((a[3] - a[1]) / row_h).floor() as usize).max(1);
        let items = v.weathers;
        let n = items.len();
        let first = v.weather_top.min(n.saturating_sub(vis));
        let bar_room = if n > vis { 14.0 * u } else { 0.0 };
        let px = (16.0 * u) as u32;
        let isz = (20.0 * u).round();
        for (k, i) in (first..n.min(first + vis)).enumerate() {
            let y = a[1] + k as f32 * row_h;
            let rc = [a[0], y, a[2] - bar_room, y + row_h];
            let on = v.weather == i;
            let hot = inside(rc, f.cursor);
            let hv = self.ease((227, "item", i), if hot || on { 1.0 } else { 0.0 }, 10.0);
            if hv > 0.0 {
                let base = if on { OPT_GROUP_ON } else { OPT_GROUP_HOT };
                self.text.rounded(r, scene, rc, ROW_R * u, fade(base, if on { 1.0 } else { hv }));
            }
            let ov = self.ease((227, "itemon", i), if on && v.zone == 0 { 1.0 } else { 0.0 }, 10.0);
            if ov > 0.0 {
                self.accent_bar(r, scene, rc, ov, false, u);
            }
            if i == 4 && k > 0 {
                self.text.rounded(r, scene, [rc[0] + 4.0 * u, y, rc[2] - 4.0 * u, y + 1.0], 0.0, [255, 255, 255, 14]);
            }
            let (icon, label) = &items[i];
            let ix = rc[0] + TEXT_IN * u;
            if let Some(ic) = self.text.icon(r, scene, icon, isz as u32, if on { ACCENT } else { mix(MUTED, WHITE, hv) }) {
                ic.place(scene, ix, y + (row_h - isz) * 0.5);
            }
            let tx = ix + isz + 14.0 * u;
            let name = clip_to(&self.text, label, px as f32, rc[2] - tx - TEXT_IN * u);
            let l = self.text.label(r, scene, &name, px, if on || hot { WHITE } else { SOFT });
            l.place(scene, tx, y + (row_h - l.h as f32) * 0.5);
            rects.push(rc);
        }
        if n > vis {
            let track = [a[2] - 6.0 * u, a[1], a[2], a[1] + vis as f32 * row_h];
            self.text.rounded(r, scene, track, 3.0 * u, OPT_TRACK);
            let th = ((vis as f32 / n as f32) * (track[3] - track[1])).max(24.0 * u);
            let ty = track[1] + (track[3] - track[1] - th) * first as f32 / (n - vis) as f32;
            self.text.rounded(r, scene, [track[0], ty, track[2], ty + th], 3.0 * u, [255, 255, 255, 60]);
        }
        vis
    }
}
