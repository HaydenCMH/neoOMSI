use super::*;

impl Ui {
    pub(super) fn draw_start_join(&mut self, r: &Renderer, scene: &mut Scene, f: &Frame, m: Metrics, v: &StartView) {
        let Metrics { w, h, u, mx, .. } = m;
        let pt = self.start.page_t;
        self.text.rounded(r, scene, [0.0, 0.0, w, h], 0.0, BG);
        let bar_h = self.draw_start_bar(r, scene, f, m, 1);
        let top = self.start_head(r, scene, m, "start.join.head", "start.join.note", bar_h + 32.0 * u, pt);
        let pad = PAD * u * 1.5;
        let cw = (640.0 * u).min(w - mx * 2.0);
        let x0 = mx;
        let card_h = (230.0 * u).round();
        let card = [x0, top, x0 + cw, top + card_h];
        self.text.rounded(r, scene, card, CARD_R * u, OPT_CARD);

        let cap = self.text.label(r, scene, "start.join.label", (13.0 * u) as u32, MUTED);
        cap.place(scene, card[0] + pad, card[1] + pad);
        let fh = (52.0 * u).round();
        let fy = card[1] + pad + cap.h as f32 + 8.0 * u;
        let field = [card[0] + pad, fy, card[2] - pad, fy + fh];
        let hot = inside(field, f.cursor);
        let hv = self.ease((216, "field", 0), if hot { 1.0 } else { 0.0 }, 10.0);
        self.text.rounded(r, scene, field, ROW_R * u, ACCENT);
        let line = (1.5 * u).round().max(1.0);
        let inner = [field[0] + line, field[1] + line, field[2] - line, field[3] - line];
        self.text.rounded(r, scene, inner, (ROW_R * u - line).max(1.0), mix(OPT_FIELD, OPT_FIELD_HOT, hv));
        let px = (18.0 * u) as u32;
        let room = field[2] - field[0] - TEXT_IN * u * 2.0;
        if v.address.is_empty() {
            let l = self.text.label(r, scene, "start.join.placeholder", px, MUTED);
            l.place(scene, field[0] + TEXT_IN * u, fy + (fh - l.h as f32) * 0.5);
            if v.caret {
                let c = self.text.label(r, scene, "|", px, WHITE);
                c.place(scene, field[0] + TEXT_IN * u - c.w as f32 * 0.5, fy + (fh - c.h as f32) * 0.5);
            }
        } else {
            let shown = format!("{}{}", v.address, if v.caret { "|" } else { "" });
            let text = clip_left(&self.text, &shown, px as f32, room);
            let l = self.text.label(r, scene, &text, px, WHITE);
            l.place(scene, field[0] + TEXT_IN * u, fy + (fh - l.h as f32) * 0.5);
        }
        let hint = self.text.label(r, scene, "start.join.hint", (13.0 * u) as u32, MUTED);
        hint.place(scene, field[0] + 4.0 * u, field[3] + 10.0 * u);

        let go_h = (52.0 * u).round();
        let go = [card[0] + pad, card[3] - pad - go_h, card[2] - pad, card[3] - pad];
        let ghot = inside(go, f.cursor);
        let gv = self.ease((216, "go", 0), if ghot { 1.0 } else { 0.0 }, 10.0);
        self.text.rounded(r, scene, go, 8.0 * u, mix(ACCENT, ACCENT_HOT, gv));
        let gl = self.text.label(r, scene, "start.join.go", (18.0 * u) as u32, ON_ACCENT);
        let gi = (24.0 * u).round();
        let total = gl.w as f32 + gi + 8.0 * u;
        let gx = go[0] + ((go[2] - go[0]) - total) * 0.5;
        if let Some(ic) = self.text.icon(r, scene, "groups", gi as u32, ON_ACCENT) {
            ic.place(scene, gx, go[1] + (go_h - gi) * 0.5);
        }
        gl.place(scene, gx + gi + 8.0 * u, go[1] + (go_h - gl.h as f32) * 0.5);
        self.start.go = go;
        self.start.field = field;
    }
}
