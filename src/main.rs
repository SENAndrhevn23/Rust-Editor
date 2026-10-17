use eframe::egui::{self, Align, Color32, FontId, Key, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rfd::FileDialog;
use rodio::{Decoder, OutputStream, Sink};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::Path;
use std::time::{Duration, Instant};

const GRID_ZOOM_VALUES: [f32; 17] = [0.25, 0.50, 0.75, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0, 96.0, 192.0];
const CAMERA_ZOOM_VALUES: [f32; 9] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

#[derive(Clone, Debug, Default)]
struct Note {
    time_ms: f64, // section-relative strum time
    lane: usize,
    sustain_ms: f64,
}

#[derive(Clone, Debug)]
struct Section {
    length_in_steps: usize,
    must_hit_section: bool,
    notes: Vec<Note>,
}
impl Default for Section {
    fn default() -> Self { Self { length_in_steps: 16, must_hit_section: false, notes: Vec::new() } }
}

#[derive(Clone, Debug)]
struct SongData {
    song: String,
    bpm: f64,
    needs_voices: bool,
    player1: String,
    player2: String,
    gf_version: String,
    speed: f64,
    mania: usize, // total lanes (1K..32K per side => 2..64)
    audio_file_path: String,
    notes: Vec<Section>,
}
impl Default for SongData {
    fn default() -> Self {
        Self { song: "Test".into(), bpm: 150.0, needs_voices: true, player1: "bf".into(), player2: "dad".into(), gf_version: "gf".into(), speed: 1.6, mania: 8, audio_file_path: String::new(), notes: vec![Section::default()] }
    }
}

#[derive(Clone)]
struct SectionClipboard { section: Section }
#[derive(Clone)]
struct AllClipboard { sections: Vec<Section> }

struct ChartEditor {
    song: SongData,
    current_section: usize,
    selected_note: Option<usize>,
    tab: String,
    grid_zoom_index: usize,
    camera_zoom_index: usize,
    camera_x_offset: f32,
    scroll_row_offset: f32,
    status: String,
    is_playing: bool,
    position_steps: f64,
    last_update: Instant,
    playback_rate: f64,
    show_grid: bool,
    disable_autoscroll: bool,
    metronome_enabled: bool,
    save_undos: bool,
    note_opacity: f32,
    low_latency_grid: bool,
    sustain_ms: f64,
    note_type: String,
    spam_density: usize,
    spam_strength: f64,
    spam_lane_from: usize,
    spam_lane_to: usize,
    ez_spam: bool,
    copied_note: Option<Note>,
    copied_section: Option<SectionClipboard>,
    copied_all: Option<AllClipboard>,
    song_credit: String,
    credit_icon: String,
    disable_note_rgb: bool,
    event_type: String,
    event_value1: String,
    event_value2: String,
    events: Vec<(String, String, String)>,
    audio_stream: Option<OutputStream>,
    audio_sink: Option<Sink>,
    audio_error: String,
    show_remove_dialog: bool,
    remove_mode: usize,
    remove_lane_from: usize,
    remove_lane_to: usize,
    remove_before_ms: f64,
    textures: Vec<egui::TextureHandle>,
}

impl ChartEditor {
    fn new(ctx: &egui::Context) -> Self {
        // Dark graphite theme: charcoal panels, slightly lighter controls, and
        // readable cool-gray text throughout the editor.
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(45, 48, 52);
        visuals.window_fill = Color32::from_rgb(48, 51, 55);
        visuals.extreme_bg_color = Color32::from_rgb(31, 33, 36);
        visuals.faint_bg_color = Color32::from_rgb(56, 59, 63);
        visuals.code_bg_color = Color32::from_rgb(36, 38, 41);
        visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(72, 76, 81));
        visuals.selection.bg_fill = Color32::from_rgb(65, 91, 109);
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(137, 184, 210));
        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(45, 48, 52);
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(65, 69, 74));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(58, 61, 66);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(78, 83, 89));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(72, 77, 83);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(106, 115, 124));
        visuals.widgets.active.bg_fill = Color32::from_rgb(62, 79, 90);
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(130, 173, 197));
        visuals.widgets.open.bg_fill = Color32::from_rgb(53, 57, 62);
        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(6.0, 5.0);
        style.spacing.button_padding = Vec2::new(7.0, 3.0);
        style.visuals.override_text_color = Some(Color32::from_rgb(225, 228, 232));
        style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(12.0));
        style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(12.0));
        style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.0));
        ctx.set_style(style);
        let textures = load_textures(ctx);
        Self {
            textures,
            song: SongData::default(), current_section: 0, selected_note: None, tab: "Charting".into(),
            grid_zoom_index: 0, camera_zoom_index: 3, camera_x_offset: 0.0, scroll_row_offset: 0.0,
            status: "Ready. Left-click adds a tap note; right-click deletes one.".into(), is_playing: false,
            position_steps: 0.0, last_update: Instant::now(), playback_rate: 1.0,
            show_grid: true, disable_autoscroll: false, metronome_enabled: false, save_undos: true,
            note_opacity: 1.0, low_latency_grid: false, sustain_ms: 0.0, note_type: "Default".into(),
            spam_density: 1, spam_strength: 1.0, spam_lane_from: 0, spam_lane_to: 7, ez_spam: false,
            copied_note: None, copied_section: None, copied_all: None,
            song_credit: "Test".into(), credit_icon: "bf".into(), disable_note_rgb: false,
            event_type: "---".into(), event_value1: String::new(), event_value2: String::new(), events: Vec::new(),
            audio_stream: None, audio_sink: None, audio_error: String::new(),
            show_remove_dialog: false, remove_mode: 0, remove_lane_from: 0, remove_lane_to: 7, remove_before_ms: 0.0,
        }
    }

    fn safe_bpm(&self) -> f64 { self.song.bpm.max(1.0) }
    fn storage_step_ms(&self) -> f64 { 15_000.0 / self.safe_bpm() }
    fn section_duration_ms(&self, index: usize) -> f64 {
        self.song.notes.get(index).map(|s| s.length_in_steps.max(1) as f64 * self.storage_step_ms()).unwrap_or(16.0 * self.storage_step_ms())
    }
    fn section_start_ms(&self, index: usize) -> f64 { (0..index).map(|i| self.section_duration_ms(i)).sum() }
    fn section_start_steps(&self, index: usize) -> f64 { self.section_start_ms(index) / self.storage_step_ms() }
    fn grid_rows(&self) -> usize { (64.0 * GRID_ZOOM_VALUES[self.grid_zoom_index]).round().max(1.0) as usize }
    fn display_step_ms(&self) -> f64 { self.section_duration_ms(self.current_section) / self.grid_rows().max(1) as f64 }
    fn camera_zoom(&self) -> f32 { CAMERA_ZOOM_VALUES[self.camera_zoom_index] }
    fn lane_width(&self) -> f32 { 60.0 * self.camera_zoom() }
    fn row_height(&self) -> f32 { 60.0 * self.camera_zoom() }
    fn total_notes(&self) -> usize { self.song.notes.iter().map(|s| s.notes.len()).sum() }
    fn selected_note(&self) -> Option<&Note> { self.selected_note.and_then(|i| self.song.notes.get(self.current_section)?.notes.get(i)) }

    fn set_status(&mut self, s: impl Into<String>) { self.status = s.into(); }
    fn set_mania_keys_per_side(&mut self, keys: usize) {
        self.song.mania = keys.clamp(1, 32) * 2;
        let lane_count = self.song.mania;
        for section in &mut self.song.notes { section.notes.retain(|n| n.lane < lane_count); }
        self.selected_note = None;
        self.camera_x_offset = self.camera_x_offset.max(0.0);
    }
    fn rescale_for_bpm_change(&mut self, old_bpm: f64, new_bpm: f64) {
        if old_bpm <= 0.0 || new_bpm <= 0.0 || (old_bpm - new_bpm).abs() < f64::EPSILON { return; }
        // Keep notes anchored to musical grid steps when the tempo changes, as in the Java editor.
        let ratio = old_bpm / new_bpm;
        for section in &mut self.song.notes {
            for note in &mut section.notes { note.time_ms *= ratio; note.sustain_ms *= ratio; }
        }
    }
    fn ensure_section(&mut self, index: usize) {
        while self.song.notes.len() <= index { self.song.notes.push(Section::default()); }
    }
    fn move_section(&mut self, delta: isize) {
        let next = (self.current_section as isize + delta).max(0) as usize;
        self.ensure_section(next);
        self.current_section = next;
        self.selected_note = None;
        self.scroll_row_offset = 0.0;
        self.position_steps = self.section_start_steps(next);
    }
    fn jump_to_playhead_section(&mut self) {
        let mut global_ms = self.position_steps * self.storage_step_ms();
        if global_ms < 0.0 { global_ms = 0.0; }
        let mut start = 0.0;
        for idx in 0..self.song.notes.len() {
            let end = start + self.section_duration_ms(idx);
            if global_ms < end || idx + 1 == self.song.notes.len() {
                if self.current_section != idx { self.current_section = idx; self.selected_note = None; }
                if !self.disable_autoscroll {
                    self.scroll_row_offset = ((global_ms - start).max(0.0) / self.display_step_ms().max(0.001)) as f32;
                    self.scroll_row_offset = self.scroll_row_offset.clamp(0.0, self.grid_rows().saturating_sub(1) as f32);
                }
                return;
            }
            start = end;
        }
    }
    fn current_section_mut(&mut self) -> &mut Section {
        self.ensure_section(self.current_section);
        &mut self.song.notes[self.current_section]
    }

    fn click_grid(&mut self, lane: usize, display_row: f64, right_click: bool) {
        if lane >= self.song.mania { return; }
        let step_ms = self.display_step_ms().max(0.0001);
        let relative_time = (display_row * step_ms).max(0.0);
        let tolerance = if right_click { (step_ms * 0.45).max(0.5) } else { (step_ms * 0.20).max(0.05) };
        let existing = self.song.notes[self.current_section].notes.iter().position(|n| n.lane == lane && (n.time_ms - relative_time).abs() <= tolerance);
        if right_click {
            if let Some(i) = existing {
                self.song.notes[self.current_section].notes.remove(i);
                self.selected_note = None;
                self.set_status(format!("Removed note in lane {lane}."));
            }
        } else if self.ez_spam {
            let strength = self.spam_strength.floor().max(1.0) as usize;
            self.spam_generate(lane, lane, self.spam_density, strength as f64, display_row as usize, true);
        } else if let Some(i) = existing {
            self.selected_note = Some(i);
            self.sustain_ms = self.song.notes[self.current_section].notes[i].sustain_ms;
            self.set_status(format!("Selected note {} in lane {}.", i + 1, lane));
        } else {
            let storage_step = self.storage_step_ms();
            let time = relative_time.min(self.section_duration_ms(self.current_section).max(0.0));
            let sec = self.current_section_mut();
            sec.notes.push(Note { time_ms: time, lane, sustain_ms: 0.0 });
            let new_index = sec.notes.len() - 1;
            self.selected_note = Some(new_index);
            self.sustain_ms = 0.0;
            self.set_status(format!("Added note in lane {lane}, at {:.2} ms.", self.section_start_ms(self.current_section) + time));
            let _ = storage_step;
        }
    }

    fn extend_selected(&mut self, full_note: bool) {
        let index = match self.selected_note { Some(i) if i < self.song.notes[self.current_section].notes.len() => i, _ => { self.set_status("Select a note first, then press E or P."); return; } };
        let increment = if full_note { self.display_step_ms() } else { self.display_step_ms() };
        self.song.notes[self.current_section].notes[index].sustain_ms += increment.max(1.0);
        self.sustain_ms = self.song.notes[self.current_section].notes[index].sustain_ms;
        self.set_status(format!("Sustain set to {:.2} ms.", self.sustain_ms));
    }

    fn spam_generate(&mut self, lane_from: usize, lane_to: usize, density: usize, strength_rows: f64, start_row: usize, visual_rows: bool) {
        let from = lane_from.min(lane_to).min(self.song.mania.saturating_sub(1));
        let to = lane_from.max(lane_to).min(self.song.mania.saturating_sub(1));
        let density = density.clamp(1, 2048);
        let step_ms = if visual_rows { self.display_step_ms() } else { self.storage_step_ms() };
        let start_ms = if visual_rows { start_row as f64 * step_ms } else { start_row as f64 * step_ms };
        let strength = strength_rows.max(0.03125);
        let end_ms = start_ms + strength * step_ms;
        let sec = self.current_section_mut();
        sec.notes.retain(|n| !(n.lane >= from && n.lane <= to && n.time_ms >= start_ms && n.time_ms < end_ms));
        let step_fraction = 1.0 / density as f64;
        let row_count = (strength * density as f64).ceil().min(100_000.0) as usize;
        for i in 0..row_count {
            let offset = i as f64 * step_fraction;
            if offset >= strength { break; }
            let time = start_ms + offset * step_ms;
            for lane in from..=to {
                sec.notes.push(Note { time_ms: time, lane, sustain_ms: 0.0 });
            }
        }
        self.selected_note = None;
        self.set_status(format!("Generated {} notes in the selected range.", row_count.saturating_mul(to - from + 1)));
    }

    fn swap_sides(&mut self) {
        for note in &mut self.song.notes[self.current_section].notes {
            if note.lane < self.song.mania / 2 { note.lane += self.song.mania / 2; }
            else { note.lane -= self.song.mania / 2; }
        }
        self.set_status("Swapped opponent and player lanes for this section.");
    }

    fn copy_selected_note(&mut self) {
        self.copied_note = self.selected_note().cloned();
        self.set_status(if self.copied_note.is_some() { "Copied selected note." } else { "Select a note before copying." });
    }
    fn paste_selected_note(&mut self) {
        if let Some(mut n) = self.copied_note.clone() {
            n.time_ms = (n.time_ms + self.display_step_ms()).min(self.section_duration_ms(self.current_section));
            self.song.notes[self.current_section].notes.push(n);
            self.selected_note = Some(self.song.notes[self.current_section].notes.len() - 1);
            self.set_status("Pasted note one visual row later.");
        } else { self.set_status("The note clipboard is empty."); }
    }
    fn copy_current_section(&mut self) {
        self.copied_section = Some(SectionClipboard { section: self.song.notes[self.current_section].clone() });
        self.set_status(format!("Copied section {} ({} notes).", self.current_section, self.song.notes[self.current_section].notes.len()));
    }
    fn paste_section_here(&mut self) {
        if let Some(c) = self.copied_section.clone() {
            self.song.notes[self.current_section] = c.section;
            self.selected_note = None;
            self.set_status("Pasted copied section here.");
        } else { self.set_status("Copy a section first."); }
    }
    fn paste_next_section(&mut self) {
        if let Some(c) = self.copied_section.clone() {
            let target = self.current_section + 1;
            self.ensure_section(target);
            self.song.notes[target] = c.section;
            self.set_status(format!("Pasted copied section into section {target}."));
        } else { self.set_status("Copy a section first."); }
    }
    fn copy_all_notes(&mut self) {
        let secs = self.song.notes.iter().filter(|s| !s.notes.is_empty()).cloned().collect::<Vec<_>>();
        self.copied_all = Some(AllClipboard { sections: secs.clone() });
        self.set_status(format!("Copied {} non-empty sections ({} notes).", secs.len(), secs.iter().map(|s| s.notes.len()).sum::<usize>()));
    }
    fn paste_all_notes(&mut self) {
        let copied = match self.copied_all.clone() { Some(c) if !c.sections.is_empty() => c, _ => { self.set_status("Copy all notes first."); return; } };
        for (offset, sec) in copied.sections.into_iter().enumerate() {
            self.ensure_section(self.current_section + offset);
            self.song.notes[self.current_section + offset] = sec;
        }
        self.set_status("Pasted copied non-empty sections from the current section.");
    }

    fn load_audio(&mut self, path: &Path) {
        self.audio_sink = None;
        self.audio_stream = None;
        match OutputStream::try_default() {
            Ok((stream, handle)) => match File::open(path).map(BufReader::new).map_err(|e| e.to_string()).and_then(|r| Decoder::new(r).map_err(|e| e.to_string())) {
                Ok(decoder) => match Sink::try_new(&handle) {
                    Ok(sink) => { sink.append(decoder); sink.pause(); self.audio_stream = Some(stream); self.audio_sink = Some(sink); self.song.audio_file_path = path.to_string_lossy().to_string(); self.audio_error.clear(); self.set_status(format!("Loaded audio: {}", path.display())); }
                    Err(e) => self.audio_error = format!("Could not create audio sink: {e}"),
                },
                Err(e) => self.audio_error = format!("Could not decode audio: {e}"),
            },
            Err(e) => self.audio_error = format!("Audio output unavailable: {e}"),
        }
        if !self.audio_error.is_empty() { self.set_status(self.audio_error.clone()); }
    }

    fn toggle_playback(&mut self) {
        self.is_playing = !self.is_playing;
        if let Some(sink) = &self.audio_sink {
            if self.is_playing {
                let seek_ms = (self.position_steps.max(0.0) * self.storage_step_ms()).max(0.0);
                let _ = sink.try_seek(Duration::from_secs_f64(seek_ms / 1000.0));
                sink.play();
            } else { sink.pause(); }
        }
        self.last_update = Instant::now();
        self.set_status(if self.is_playing { "Playback started." } else { "Playback paused." });
    }

    fn save_chart_dialog(&mut self) {
        let chosen = FileDialog::new().set_title("Save FNF Chart").add_filter("FNF Chart JSON", &["json"]).add_filter("FNF Editor Binary", &["bin"]).set_file_name(&format!("{}.json", safe_filename(&self.song.song))).save_file();
        if let Some(path) = chosen {
            let binary = path.extension().map(|e| e.to_string_lossy().eq_ignore_ascii_case("bin")).unwrap_or(false);
            let result = if binary { self.save_binary(&path) } else { self.save_json(&path) };
            match result { Ok(()) => self.set_status(format!("Saved chart to {} ({} notes).", path.display(), self.total_notes())), Err(e) => self.set_status(format!("Save failed: {e}")) }
        }
    }
    fn save_json(&self, path: &Path) -> Result<(), String> {
        let mut sections = Vec::with_capacity(self.song.notes.len());
        let mut start_ms = 0.0;
        for sec in &self.song.notes {
            let mut tuples: Vec<Value> = sec.notes.iter().filter(|n| n.lane < self.song.mania).map(|n| json!([round3(start_ms + n.time_ms), n.lane, round3(n.sustain_ms)])).collect();
            // The Java source preserves section/note insertion order; preserve that here too.
            tuples.shrink_to_fit();
            sections.push(json!({ "lengthInSteps": sec.length_in_steps.max(1), "mustHitSection": sec.must_hit_section, "sectionNotes": tuples }));
            start_ms += sec.length_in_steps.max(1) as f64 * self.storage_step_ms();
        }
        let root = json!({ "song": {
            "song": self.song.song, "bpm": self.safe_bpm(), "needsVoices": self.song.needs_voices,
            "player1": self.song.player1, "player2": self.song.player2, "gfVersion": self.song.gf_version,
            "speed": self.song.speed, "mania": self.song.mania.clamp(2, 64), "notes": sections,
            "generatedBy": "SNIFF ver.6"
        }});
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(&mut file, &root).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())
    }
    fn save_binary(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension(format!("{}.saving", path.extension().unwrap_or_default().to_string_lossy()));
        let mut out = File::create(&tmp).map_err(|e| e.to_string())?;
        write_bin_string(&mut out, "FNFEBIN3")?; write_bin_string(&mut out, &self.song.song)?;
        out.write_all(&self.safe_bpm().to_be_bytes()).map_err(|e| e.to_string())?;
        out.write_all(&[u8::from(self.song.needs_voices)]).map_err(|e| e.to_string())?;
        write_bin_string(&mut out, &self.song.player1)?; write_bin_string(&mut out, &self.song.player2)?; write_bin_string(&mut out, &self.song.gf_version)?;
        out.write_all(&self.song.speed.to_be_bytes()).map_err(|e| e.to_string())?;
        out.write_all(&(self.song.mania.clamp(2, 64) as i32).to_be_bytes()).map_err(|e| e.to_string())?;
        let mut start_ms = 0.0;
        for sec in &self.song.notes {
            let valid: Vec<&Note> = sec.notes.iter().filter(|n| n.lane < self.song.mania).collect();
            out.write_all(&(sec.length_in_steps.max(1) as i32).to_be_bytes()).map_err(|e| e.to_string())?;
            out.write_all(&[u8::from(sec.must_hit_section)]).map_err(|e| e.to_string())?;
            out.write_all(&(valid.len() as i64).to_be_bytes()).map_err(|e| e.to_string())?;
            for n in valid { out.write_all(&(start_ms + n.time_ms).to_be_bytes()).map_err(|e| e.to_string())?; out.write_all(&(n.lane as i32).to_be_bytes()).map_err(|e| e.to_string())?; out.write_all(&n.sustain_ms.to_be_bytes()).map_err(|e| e.to_string())?; }
            start_ms += sec.length_in_steps.max(1) as f64 * self.storage_step_ms();
        }
        out.flush().map_err(|e| e.to_string())?; drop(out);
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    fn load_chart_dialog(&mut self) {
        if let Some(path) = FileDialog::new().set_title("Load FNF Chart (JSON / BIN)").add_filter("FNF Charts", &["json", "bin"]).pick_file() {
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            let loaded = if ext == "bin" { load_binary(&path) } else { load_json(&path) };
            match loaded {
                Ok(song) => { self.song = song; self.current_section = 0; self.selected_note = None; self.position_steps = 0.0; self.scroll_row_offset = 0.0; self.camera_x_offset = 0.0; self.sustain_ms = 0.0; self.set_status(format!("Loaded {} ({} sections, {} notes).", path.display(), self.song.notes.len(), self.total_notes())); }
                Err(e) => self.set_status(format!("Load failed: {e}")),
            }
        }
    }

    fn minify_json_dialog(&mut self) {
        if let Some(path) = FileDialog::new().set_title("Select JSON File to Minify").add_filter("JSON", &["json"]).pick_file() {
            let result = fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Value>(&s).map_err(|e| e.to_string())).and_then(|v| serde_json::to_vec(&v).map_err(|e| e.to_string())).and_then(|bytes| fs::write(&path, bytes).map_err(|e| e.to_string()));
            self.set_status(match result { Ok(()) => format!("Minified {}.", path.display()), Err(e) => format!("Minify failed: {e}") });
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() { return; }
        let keys = ctx.input(|i| (
            i.key_pressed(Key::Space), i.key_pressed(Key::E), i.key_pressed(Key::P), i.key_pressed(Key::Z), i.key_pressed(Key::X),
            i.key_pressed(Key::N), i.key_pressed(Key::M), i.key_pressed(Key::R), i.key_pressed(Key::J), i.key_pressed(Key::K),
            i.key_pressed(Key::I), i.key_pressed(Key::L), i.key_pressed(Key::W), i.key_pressed(Key::S), i.key_pressed(Key::A), i.key_pressed(Key::D)
        ));
        if keys.0 { self.toggle_playback(); }
        if keys.1 { self.extend_selected(true); }
        if keys.2 { self.extend_selected(false); }
        if keys.3 { self.grid_zoom_index = self.grid_zoom_index.saturating_sub(1); }
        if keys.4 { self.grid_zoom_index = (self.grid_zoom_index + 1).min(GRID_ZOOM_VALUES.len() - 1); }
        if keys.5 { self.camera_zoom_index = (self.camera_zoom_index + 1).min(CAMERA_ZOOM_VALUES.len() - 1); }
        if keys.6 { self.camera_zoom_index = self.camera_zoom_index.saturating_sub(1); }
        if keys.7 { self.swap_sides(); }
        if keys.8 { self.camera_x_offset = (self.camera_x_offset - self.lane_width()).max(0.0); }
        if keys.9 { self.scroll_row_offset = (self.scroll_row_offset + 1.0).min(self.grid_rows() as f32 - 1.0); }
        if keys.10 { self.scroll_row_offset = (self.scroll_row_offset - 1.0).max(0.0); }
        if keys.11 { self.camera_x_offset += self.lane_width(); }
        if keys.12 { self.position_steps = (self.position_steps - 1.0).max(0.0); self.jump_to_playhead_section(); }
        if keys.13 { self.position_steps += 1.0; self.jump_to_playhead_section(); }
        if keys.14 { self.move_section(-1); }
        if keys.15 { self.move_section(1); }
    }

    fn draw_grid(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size().max(Vec2::new(250.0, 300.0));
        let (rect, response) = ui.allocate_exact_size(available, Sense::click());
        let painter = ui.painter_at(rect);
        let canvas_bg = Color32::from_rgb(39, 42, 46);
        painter.rect_filled(rect, 0.0, canvas_bg);

        // These margins and cell sizes mirror ChartGridPanel.gridStartX/gridStartY
        // and effectiveLaneWidth/effectiveRowHeight in the Java source.
        let header_h = 60.0;
        let left_margin = 50.0;
        let lane_w = self.lane_width();
        let row_h = self.row_height();
        let lane_count = self.song.mania.clamp(2, 64);
        let grid_left = rect.left() + left_margin;
        let grid_top = rect.top() + header_h;
        let viewport_right = (rect.right() - 2.0).max(grid_left + 1.0);
        let world_right = grid_left + lane_count as f32 * lane_w;
        let grid_right = world_right.min(viewport_right);
        let grid_rect = Rect::from_min_max(
            Pos2::new(grid_left, grid_top.min(rect.bottom() - 1.0)),
            Pos2::new(grid_right.max(grid_left + 1.0), (rect.bottom() - 1.0).max(grid_top + 1.0)),
        );

        let section_notes = &self.song.notes[self.current_section].notes;
        let total_notes = self.total_notes();
        painter.text(
            Pos2::new(rect.left() + 20.0, rect.top() + 8.0),
            egui::Align2::LEFT_TOP,
            format!(
                "Section: {} | Camera: {:.2} | Status: {} | Notes: {}",
                self.current_section,
                self.scroll_row_offset,
                if self.is_playing { "PLAYING" } else { "PAUSED" },
                total_notes
            ),
            FontId::proportional(12.0),
            Color32::from_rgb(225, 228, 232),
        );
        painter.text(
            Pos2::new(rect.left() + 20.0, rect.top() + 26.0),
            egui::Align2::LEFT_TOP,
            format!("Mania: {}K ({} total lanes)", lane_count / 2, lane_count),
            FontId::proportional(12.0),
            Color32::from_rgb(225, 228, 232),
        );

        // Paint the source grid tiles with dark-gray tints. Every
        // four visual rows switches tile, just as ChartGridPanel.paintComponent does.
        let clipped = painter.with_clip_rect(grid_rect);
        let visible_rows = ((grid_rect.height() / row_h).ceil() as usize + 2).min(self.grid_rows());
        let first_row = self.scroll_row_offset.floor().max(0.0) as usize;
        let last_row = (first_row + visible_rows).min(self.grid_rows());
        for row in first_row..last_row {
            let y = grid_rect.top() + (row as f32 - self.scroll_row_offset) * row_h;
            let cell_h = row_h;
            let grid_texture = if (row / 4) % 2 == 0 { self.textures.get(2) } else { self.textures.get(3) };
            for lane in 0..lane_count {
                let x = grid_left + lane as f32 * lane_w - self.camera_x_offset;
                let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::new(lane_w, cell_h));
                if cell.right() < grid_rect.left() || cell.left() > grid_rect.right() {
                    continue;
                }
                if self.show_grid {
                    if let Some(texture) = grid_texture {
                        // Keep the source grid tiles, but tint them to two graphite
                        // shades instead of the original white/light-gray colors.
                        let tint = if (row / 4) % 2 == 0 {
                            Color32::from_gray(96)
                        } else {
                            Color32::from_gray(85)
                        };
                        clipped.image(
                            texture.id(),
                            cell,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            tint,
                        );
                    } else {
                        let fill = if (row / 4) % 2 == 0 { Color32::from_gray(72) } else { Color32::from_gray(85) };
                        clipped.rect_filled(cell, 0.0, fill);
                    }
                    clipped.rect_stroke(cell, 0.0, Stroke::new(0.75, Color32::from_rgb(35, 37, 40)), egui::StrokeKind::Inside);
                }
            }
        }

        // Blue divider between the two sides for every supported key mode (1K–32K per side).
        // The split is always at the midpoint of the active lane range, not hard-coded to 4K.
        let separator_x = grid_left + (lane_count as f32 / 2.0) * lane_w - self.camera_x_offset;
        clipped.line_segment(
            [Pos2::new(separator_x, grid_rect.top()), Pos2::new(separator_x, grid_rect.bottom())],
            Stroke::new(3.0, Color32::from_rgb(50, 145, 255)),
        );

        // Character icons sit directly above lanes 2 and 6, at the original 50x50 size.
        if lane_count == 8 && self.textures.len() >= 2 {
            let left_texture = if self.song.notes[self.current_section].must_hit_section { &self.textures[0] } else { &self.textures[1] };
            let right_texture = if self.song.notes[self.current_section].must_hit_section { &self.textures[1] } else { &self.textures[0] };
            for (x, texture) in [
                (grid_left + 2.0 * lane_w - self.camera_x_offset, left_texture),
                (grid_left + 6.0 * lane_w - self.camera_x_offset, right_texture),
            ] {
                let dest = Rect::from_center_size(Pos2::new(x, grid_top - 25.0), Vec2::new(50.0, 50.0));
                painter.image(texture.id(), dest, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
        }

        let step_ms = self.display_step_ms().max(0.0001);
        let alpha = (self.note_opacity * 255.0).clamp(0.0, 255.0) as u8;
        for (idx, note) in section_notes.iter().enumerate() {
            if note.lane >= lane_count {
                continue;
            }
            let display_row = (note.time_ms / step_ms) as f32;
            let y = grid_top + (display_row - self.scroll_row_offset) * row_h;
            let x = grid_left + note.lane as f32 * lane_w - self.camera_x_offset;
            let note_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(lane_w, row_h));
            if note_rect.bottom() < grid_rect.top() || note_rect.top() > grid_rect.bottom()
                || note_rect.right() < grid_rect.left() || note_rect.left() > grid_rect.right() {
                continue;
            }
            if note.sustain_ms > 0.0 {
                let sustain_grids = note.sustain_ms / step_ms;
                let tail_height = ((sustain_grids - 0.5).max(0.0) as f32 * row_h).max(0.0);
                let body_width = (lane_w / 3.0).max(1.0);
                let tail = Rect::from_min_size(
                    Pos2::new(x + body_width, y + row_h * 0.5),
                    Vec2::new(body_width, tail_height),
                );
                clipped.rect_filled(tail, 0.0, Color32::from_rgba_unmultiplied(0, 255, 0, 150));
            }
            if self.textures.len() >= 8 {
                let arrow_tex = &self.textures[4 + (note.lane % 4)];
                let tint = if Some(idx) == self.selected_note {
                    Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
                } else {
                    Color32::from_white_alpha(alpha)
                };
                clipped.image(arrow_tex.id(), note_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
            } else {
                clipped.rect_filled(note_rect, 0.0, Color32::from_white_alpha(alpha));
            }
        }

        if self.is_playing {
            let section_global_start = self.section_start_ms(self.current_section);
            let playhead_row = ((self.position_steps * self.storage_step_ms() - section_global_start).max(0.0) / step_ms) as f32;
            let y = grid_top + (playhead_row - self.scroll_row_offset) * row_h;
            if y >= grid_rect.top() && y <= grid_rect.bottom() {
                clipped.line_segment(
                    [Pos2::new(grid_rect.left(), y), Pos2::new(grid_rect.right(), y)],
                    Stroke::new(3.0, Color32::YELLOW),
                );
            }
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let max_scroll = self.grid_rows().saturating_sub(1) as f32;
                self.scroll_row_offset = (self.scroll_row_offset - scroll / row_h).clamp(0.0, max_scroll);
            }
        }
        if response.clicked() || response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if grid_rect.contains(pos) {
                    let lane = ((pos.x - grid_left + self.camera_x_offset) / lane_w).floor().max(0.0) as usize;
                    let row = ((pos.y - grid_top) / row_h + self.scroll_row_offset).floor().max(0.0) as f64;
                    if lane < lane_count {
                        self.click_grid(lane, row, response.secondary_clicked());
                    }
                }
            }
        }
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_rgb(75, 79, 84)), egui::StrokeKind::Inside);
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let tabs = ["Charting", "Data", "Events", "Note", "Spamming", "Optimiser", "Section", "Song", "Controls"];
        ui.horizontal(|ui| {
            for tab in tabs {
                let selected = self.tab == tab;
                let response = ui.selectable_label(selected, tab);
                // Thin neutral borders keep the tabs visible against the dark theme.
                ui.painter().rect_stroke(
                    response.rect,
                    0.0,
                    Stroke::new(0.8, Color32::from_rgb(81, 86, 92)),
                    egui::StrokeKind::Inside,
                );
                if response.clicked() { self.tab = tab.into(); }
            }
        });
        ui.separator();
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        self.tab_bar(ui);
        let active_tab = self.tab.clone();
        match active_tab.as_str() {
            "Charting" => {
                let content = ui.available_rect_before_wrap();
                let margin = 10.0;
                let row_height = ((content.height() - margin * 2.0) / 6.0).max(30.0);
                let row_width = (content.width() - margin * 2.0).max(80.0);
                let mut y = content.top() + margin;
                let make_row = |y: f32, h: f32| Rect::from_min_size(
                    Pos2::new(content.left() + margin, y + ((h - 26.0) * 0.5).max(0.0)),
                    Vec2::new(row_width, 26.0));
                ui.put(make_row(y, row_height), egui::Checkbox::new(&mut self.metronome_enabled, RichText::new("Metronome Enabled").strong())); y += row_height;
                ui.put(make_row(y, row_height), egui::Checkbox::new(&mut self.disable_autoscroll, RichText::new("Disable Autoscroll").strong())); y += row_height;
                ui.put(make_row(y, row_height), egui::Checkbox::new(&mut self.show_grid, RichText::new("Show Grid").strong())); y += row_height;
                ui.put(make_row(y, row_height), egui::Checkbox::new(&mut self.save_undos, RichText::new("Save Undos").strong())); y += row_height;
                ui.put(make_row(y, row_height), egui::Label::new(RichText::new("Playback Rate:").strong())); y += row_height;
                ui.put(make_row(y, row_height), egui::Slider::new(&mut self.playback_rate, 0.25..=4.0).suffix("×"));
            }
            "Data" => {
                ui.label("Song Credit:"); ui.text_edit_singleline(&mut self.song_credit);
                ui.label("Credit Icon:"); ui.text_edit_singleline(&mut self.credit_icon);
                ui.checkbox(&mut self.disable_note_rgb, "Disable Note RGB");
                ui.label("These fields mirror the original editor's auxiliary data tab.");
            }
            "Events" => {
                egui::ComboBox::from_label("Event").selected_text(&self.event_type).show_ui(ui, |ui| { for s in ["---", "Hey!", "Camera Flash", "Play Animation"] { ui.selectable_value(&mut self.event_type, s.to_string(), s); } });
                ui.label("Value 1:"); ui.text_edit_singleline(&mut self.event_value1);
                ui.label("Value 2:"); ui.text_edit_singleline(&mut self.event_value2);
                if ui.button("Add Event").clicked() { self.events.push((self.event_type.clone(), self.event_value1.clone(), self.event_value2.clone())); self.set_status("Added an editor event (not serialized by the original chart format)."); }
                ui.separator(); for (i, ev) in self.events.iter().enumerate() { ui.label(format!("{}. {} | {} | {}", i+1, ev.0, ev.1, ev.2)); }
            }
            "Note" => {
                ui.label("Sustain Length (ms):");
                let mut selected_sustain = self.selected_note().map(|n| n.sustain_ms).unwrap_or(self.sustain_ms);
                if ui.add(egui::DragValue::new(&mut selected_sustain).speed(10.0).range(0.0..=500_000.0)).changed() {
                    self.sustain_ms = selected_sustain;
                    if let Some(i) = self.selected_note { if let Some(note) = self.song.notes[self.current_section].notes.get_mut(i) { note.sustain_ms = selected_sustain; } }
                }
                ui.label("Strum Time (ms):");
                let mut strum_time = self.selected_note().map(|n| format!("{:.2}", self.section_start_ms(self.current_section) + n.time_ms)).unwrap_or_else(|| "—".into());
                ui.add_enabled(false, egui::TextEdit::singleline(&mut strum_time));
                egui::ComboBox::from_label("Note Type").selected_text(&self.note_type).show_ui(ui, |ui| { for s in ["Default", "Alt Animation", "Mine", "Hurt Note"] { ui.selectable_value(&mut self.note_type, s.to_string(), s); } });
                ui.horizontal(|ui| { if ui.button("Copy Notes").clicked() { self.copy_selected_note(); } if ui.button("Paste Notes").clicked() { self.paste_selected_note(); } });
                if let Some(i) = self.selected_note { ui.label(format!("Selected note: {}", i + 1)); } else { ui.label("No note selected."); }
                ui.label("E: make / extend long note; P: extend by one grid row.");
            }
            "Spamming" => {
                ui.label("Spam Density (notes per grid row):"); ui.add(egui::DragValue::new(&mut self.spam_density).range(1..=2048));
                ui.label("Spam Strength (grid rows to fill):"); ui.add(egui::DragValue::new(&mut self.spam_strength).speed(0.25).range(0.03125..=4096.0));
                ui.label("Lane range for spam:");
                ui.horizontal(|ui| { ui.label("From"); ui.add(egui::DragValue::new(&mut self.spam_lane_from).range(0..=self.song.mania.saturating_sub(1))); ui.label("To"); ui.add(egui::DragValue::new(&mut self.spam_lane_to).range(0..=self.song.mania.saturating_sub(1))); });
                if ui.button("Spam Notes Across Grids").clicked() { self.spam_generate(self.spam_lane_from, self.spam_lane_to, self.spam_density, self.spam_strength, 0, false); }
                ui.checkbox(&mut self.ez_spam, "Enable EZ Spam Mode");
                if ui.button("Stretch Notes (×2 sustain)").clicked() { for n in &mut self.song.notes[self.current_section].notes { n.sustain_ms *= 2.0; } self.set_status("Doubled sustain lengths in this section."); }
                if ui.button("Shift Notes (+1 storage step)").clicked() { let delta = self.storage_step_ms(); for n in &mut self.song.notes[self.current_section].notes { n.time_ms += delta; } self.set_status("Shifted notes one storage step later."); }
                if ui.button("Duplicate Notes (+1 section half)").clicked() { let originals = self.song.notes[self.current_section].notes.clone(); let shift = self.section_duration_ms(self.current_section) / 2.0; self.song.notes[self.current_section].notes.extend(originals.into_iter().map(|mut n| { n.time_ms += shift; n })); self.set_status("Duplicated notes into the second half of this section."); }
                ui.label("EZ Spam uses the clicked lane and fills the selected number of visual rows.");
            }
            "Optimiser" => {
                ui.label("Note Rendering Transparency (Optimisation):"); ui.add(egui::Slider::new(&mut self.note_opacity, 0.0..=1.0).show_value(true));
                ui.checkbox(&mut self.low_latency_grid, "Low-Latency Grid Rendering Mode");
            }
            "Section" => {
                let mut must_hit = self.song.notes[self.current_section].must_hit_section;
                if ui.checkbox(&mut must_hit, "Must Hit Section (BF Camera Focus)").changed() { self.song.notes[self.current_section].must_hit_section = must_hit; }
                if ui.button("Clear Section").clicked() { self.song.notes[self.current_section].notes.clear(); self.selected_note = None; self.set_status("Cleared current section."); }
                if ui.button("Clear Opponent Side (first half)").clicked() { self.song.notes[self.current_section].notes.retain(|n| n.lane >= self.song.mania / 2); self.selected_note = None; }
                if ui.button("Clear Player Side (second half)").clicked() { self.song.notes[self.current_section].notes.retain(|n| n.lane < self.song.mania / 2); self.selected_note = None; }
                if ui.button("Remove Notes From Chart…").clicked() { self.show_remove_dialog = true; }
                if ui.button("Swap Section Sides (R)").clicked() { self.swap_sides(); }
                ui.separator();
                if ui.button("Copy Notes Section").clicked() { self.copy_current_section(); }
                if ui.button("Copy All Notes").clicked() { self.copy_all_notes(); }
                if ui.button("Paste All Notes").clicked() { self.paste_all_notes(); }
                if ui.button("Paste Notes Section Here").clicked() { self.paste_section_here(); }
                if ui.button("Paste Next Notes Section").clicked() { self.paste_next_section(); }
                ui.separator();
                ui.label(format!("Current section: {}", self.current_section));
                ui.label(format!("Section length: {} steps", self.song.notes[self.current_section].length_in_steps));
                ui.horizontal(|ui| { if ui.button("− 1 step").clicked() { self.song.notes[self.current_section].length_in_steps = self.song.notes[self.current_section].length_in_steps.saturating_sub(1).max(1); } if ui.button("+ 1 step").clicked() { self.song.notes[self.current_section].length_in_steps += 1; } });
            }
            "Song" => {
                egui::Grid::new("song-grid").num_columns(2).spacing([8.0, 7.0]).show(ui, |ui| {
                    ui.label("Song:"); ui.text_edit_singleline(&mut self.song.song); ui.end_row();
                    ui.label("BPM:");
                    let old_bpm = self.song.bpm;
                    if ui.add(egui::DragValue::new(&mut self.song.bpm).range(1.0..=999999.0).speed(1.0)).changed() {
                        self.rescale_for_bpm_change(old_bpm, self.song.bpm.max(1.0));
                    }
                    ui.end_row();
                    ui.label("Mania (keys per side):"); let mut keys = (self.song.mania / 2).clamp(1, 32); if ui.add(egui::DragValue::new(&mut keys).range(1..=32)).changed() { self.set_mania_keys_per_side(keys); } ui.end_row();
                    ui.label("Speed:"); ui.add(egui::DragValue::new(&mut self.song.speed).range(0.1..=1_000_000.0).speed(0.1)); ui.end_row();
                    ui.label("Boyfriend:"); ui.text_edit_singleline(&mut self.song.player1); ui.end_row();
                    ui.label("Daddy Dearest:"); ui.text_edit_singleline(&mut self.song.player2); ui.end_row();
                    ui.label("Girlfriend:"); ui.text_edit_singleline(&mut self.song.gf_version); ui.end_row();
                    ui.label("Voices:"); ui.checkbox(&mut self.song.needs_voices, "Has voices"); ui.end_row();
                });
                if ui.button("Import Audio Track").clicked() { if let Some(path) = FileDialog::new().set_title("Select Song Audio").add_filter("Audio", &["wav", "mp3", "ogg", "flac"]).pick_file() { self.load_audio(&path); } }
                ui.label(if self.song.audio_file_path.is_empty() { "No audio loaded".into() } else { format!("Loaded: {}", Path::new(&self.song.audio_file_path).file_name().unwrap_or_default().to_string_lossy()) });
                if !self.audio_error.is_empty() { ui.colored_label(Color32::LIGHT_RED, &self.audio_error); }
                if ui.button("Save Chart (JSON / BIN)").clicked() { self.save_chart_dialog(); }
                if ui.button("Load Chart (JSON / BIN)").clicked() { self.load_chart_dialog(); }
                if ui.button("Minify JSON").clicked() { self.minify_json_dialog(); }
                ui.label("JSON exports use the Psych-style nested { song: { ... } } layout; BIN uses FNFEBIN3.");
            }
            _ => {
                ui.heading("Keyboard Controls");
                ui.label("SPACE — Start / pause playback"); ui.label("P — Extend selected note by one visual grid interval"); ui.label("E — Make selected note long / extend it");
                ui.label("Z / X — Grid zoom out / in"); ui.label("N / M — Camera zoom in / out"); ui.label("J / K / I / L — Camera left / down / up / right");
                ui.label("W / S — Move playback by one storage step"); ui.label("A / D — Previous / next section"); ui.label("R — Swap section sides");
                ui.label("Left click — Place/select note; right click — Delete note; mouse wheel — Scroll grid.");
                ui.separator(); ui.label("Rust Editor");
            }
        }
    }
}

impl eframe::App for ChartEditor {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f64();
        self.last_update = now;
        if self.is_playing {
            self.position_steps += dt * (self.safe_bpm() / 15.0) * self.playback_rate;
            self.jump_to_playhead_section();
            if let Some(sink) = &self.audio_sink { if sink.empty() { self.is_playing = false; } }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.handle_shortcuts(ctx);

        // The reference uses a ~150px bottom JTextArea and a 42/58 split pane.
        egui::TopBottomPanel::bottom("status-bar")
            .resizable(true)
            .default_height(150.0)
            .min_height(145.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(39, 42, 46)).inner_margin(egui::Margin::same(5)))
            .show(ctx, |ui| {
                let lane_half = self.song.mania / 2;
                let mut opponent = 0usize;
                let mut player = 0usize;
                for sec in &self.song.notes {
                    for n in &sec.notes {
                        if n.lane < lane_half { opponent += 1; } else { player += 1; }
                    }
                }
                let rendered = self.song.notes[self.current_section].notes.iter().filter(|n| n.lane < self.song.mania).count();
                let status_text = format!(
                    "E - Make Selected Note Long; press again to extend by 1 grid\nZ/X - Zoom Grid Out / In\nW/S - Move 1 Step     A/D - Prev/Next Section\nMouse Wheel - Scroll Through Grid\nLeft Click - Place Note     Right Click - Delete Note\n\nOpponent: {}\nPlayer: {}\nTotal Notes: {}\nRendered Notes: {}",
                    opponent, player, self.total_notes(), rendered
                );
                ui.label(RichText::new(status_text).monospace().size(12.0).color(Color32::WHITE));
            });

        let screen_width = ctx.screen_rect().width();
        let control_width = (screen_width * 0.58).clamp(410.0, 1200.0);
        egui::SidePanel::right("control-panel")
            .resizable(true)
            .default_width(control_width)
            .min_width(360.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(45, 48, 52)).inner_margin(egui::Margin::same(0)))
            .show(ctx, |ui| {
                self.right_panel(ui);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(45, 48, 52)).inner_margin(egui::Margin::same(0)))
            .show(ctx, |ui| {
                self.draw_grid(ui);
            });

        if self.show_remove_dialog {
            egui::Window::new("Remove Notes From Chart").collapsible(false).resizable(false).show(ctx, |ui| {
                egui::ComboBox::from_label("Removal range").selected_text(match self.remove_mode { 0 => "Remove all chart notes", 1 => "Remove notes before time", _ => "Remove lane range in all sections" }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.remove_mode, 0, "Remove all chart notes"); ui.selectable_value(&mut self.remove_mode, 1, "Remove notes before time"); ui.selectable_value(&mut self.remove_mode, 2, "Remove lane range in all sections");
                });
                if self.remove_mode == 1 { ui.add(egui::DragValue::new(&mut self.remove_before_ms).prefix("Milliseconds: ").range(0.0..=1_000_000_000.0)); }
                if self.remove_mode == 2 { ui.horizontal(|ui| { ui.label("Lane from"); ui.add(egui::DragValue::new(&mut self.remove_lane_from).range(0..=self.song.mania.saturating_sub(1))); ui.label("to"); ui.add(egui::DragValue::new(&mut self.remove_lane_to).range(0..=self.song.mania.saturating_sub(1))); }); }
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        let bpm_step = self.storage_step_ms(); let mut removed = 0usize;
                        let mut global_start = 0.0;
                        for sec in &mut self.song.notes {
                            sec.notes.retain(|n| {
                                let global_t = global_start + n.time_ms;
                                let keep = match self.remove_mode { 0 => false, 1 => global_t >= self.remove_before_ms, _ => !(n.lane >= self.remove_lane_from.min(self.remove_lane_to) && n.lane <= self.remove_lane_from.max(self.remove_lane_to)) };
                                if !keep { removed += 1; } keep
                            });
                            global_start += sec.length_in_steps as f64 * bpm_step;
                        }
                        self.selected_note = None; self.show_remove_dialog = false; self.set_status(format!("Removed {removed} notes."));
                    }
                    if ui.button("Cancel").clicked() { self.show_remove_dialog = false; }
                });
            });
        }
    }
}

fn load_textures(ctx: &egui::Context) -> Vec<egui::TextureHandle> {
    let sources: [(&str, &[u8]); 8] = [
        ("bf", include_bytes!("../assets/icons/bf.png")),
        ("dad", include_bytes!("../assets/icons/dad.png")),
        ("grid-grey", include_bytes!("../assets/charteditor/GridGrey.png")),
        ("grid-white", include_bytes!("../assets/charteditor/GridWhite.png")),
        ("note-left", include_bytes!("../assets/notes/LeftComing.png")),
        ("note-down", include_bytes!("../assets/notes/DownComing.png")),
        ("note-up", include_bytes!("../assets/notes/UpComing.png")),
        ("note-right", include_bytes!("../assets/notes/RightComing.png")),
    ];
    let mut out = Vec::new();
    for (name, bytes) in sources {
        if let Ok(image) = image::load_from_memory(bytes) {
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            out.push(ctx.load_texture(name, color, egui::TextureOptions::LINEAR));
        }
    }
    out
}

fn zoom_label(v: f32) -> String {
    if v == 0.25 { "1/0.25".into() } else if v == 0.5 { "1/0.50".into() } else if v == 0.75 { "1/0.75".into() } else if v.fract() == 0.0 { format!("1/{}", v as usize) } else { format!("1/{v}") }
}
fn safe_filename(s: &str) -> String { let cleaned = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect::<String>(); if cleaned.trim_matches('-').is_empty() { "chart".into() } else { cleaned } }
fn round3(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }
fn write_bin_string(out: &mut impl Write, s: &str) -> Result<(), String> { let b = s.as_bytes(); let len = i32::try_from(b.len()).map_err(|_| "String too long for BIN format".to_string())?; out.write_all(&len.to_be_bytes()).map_err(|e| e.to_string())?; out.write_all(b).map_err(|e| e.to_string()) }

fn load_json(path: &Path) -> Result<SongData, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_chart_json(&text)
}
fn json_get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> { v.get(k) }
fn as_str(v: Option<&Value>) -> Option<String> { v.and_then(Value::as_str).map(ToOwned::to_owned) }
fn as_f64(v: Option<&Value>) -> Option<f64> { v.and_then(Value::as_f64) }
fn as_bool(v: Option<&Value>) -> Option<bool> { v.and_then(Value::as_bool) }
fn find_deep_notes<'a>(v: &'a Value) -> Option<&'a Value> {
    if let Some(x) = v.get("notes") { if x.is_array() { return Some(x); } }
    match v { Value::Object(m) => m.values().find_map(find_deep_notes), Value::Array(a) => a.iter().find_map(find_deep_notes), _ => None }
}
fn parse_chart_json(text: &str) -> Result<SongData, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let container = root.get("song").filter(|v| v.is_object()).unwrap_or(&root);
    let mut song = SongData::default();
    if let Some(s) = as_str(json_get(container, "song")).or_else(|| as_str(json_get(&root, "song"))) { if !s.is_empty() { song.song = s; } }
    if let Some(x) = as_f64(json_get(container, "bpm")).or_else(|| as_f64(json_get(&root, "bpm"))) { if x > 0.0 { song.bpm = x; } }
    if let Some(x) = as_bool(json_get(container, "needsVoices")).or_else(|| as_bool(json_get(&root, "needsVoices"))) { song.needs_voices = x; }
    for (field, dest) in [("player1", &mut song.player1), ("player2", &mut song.player2), ("gfVersion", &mut song.gf_version)] {
        if let Some(s) = as_str(json_get(container, field)).or_else(|| as_str(json_get(&root, field))) { if !s.is_empty() { *dest = s; } }
    }
    if let Some(x) = as_f64(json_get(container, "speed")).or_else(|| as_f64(json_get(&root, "speed"))) { if x > 0.0 { song.speed = x; } }
    if let Some(x) = as_f64(json_get(container, "mania")).or_else(|| as_f64(json_get(&root, "mania"))) { if x > 0.0 { song.mania = (x.round() as usize).clamp(2, 64); } }
    let notes_val = json_get(container, "notes").or_else(|| json_get(&root, "notes")).or_else(|| find_deep_notes(&root));
    let mut sections = Vec::new(); let mut start_ms = 0.0; let step_ms = 15_000.0 / song.bpm.max(1.0);
    if let Some(array) = notes_val.and_then(Value::as_array) {
        for section_val in array {
            if !section_val.is_object() { continue; }
            let length = as_f64(section_val.get("lengthInSteps")).filter(|x| *x > 0.0).or_else(|| as_f64(section_val.get("sectionBeats")).filter(|x| *x > 0.0).map(|x| x * 4.0)).unwrap_or(16.0).round().max(1.0) as usize;
            let must_hit = as_bool(section_val.get("mustHitSection")).unwrap_or(false);
            let mut section = Section { length_in_steps: length, must_hit_section: must_hit, notes: Vec::new() };
            if let Some(note_array) = section_val.get("sectionNotes").and_then(Value::as_array) {
                for note_value in note_array {
                    let tuple = match note_value.as_array() { Some(a) if a.len() >= 3 => a, _ => continue };
                    let global_time = tuple[0].as_f64().unwrap_or(0.0);
                    let lane_number = tuple[1].as_f64().unwrap_or(-1.0).round();
                    let sustain = tuple[2].as_f64().unwrap_or(0.0).max(0.0);
                    if lane_number >= 0.0 && lane_number < song.mania as f64 {
                        let lane = lane_number as usize;
                        section.notes.push(Note { time_ms: (global_time - start_ms).max(0.0), lane, sustain_ms: sustain });
                    }
                }
            }
            start_ms += length as f64 * step_ms;
            sections.push(section);
        }
    }
    if !sections.is_empty() { song.notes = sections; }
    Ok(song)
}

struct BinCursor { data: Vec<u8>, pos: usize }
impl BinCursor {
    fn take(&mut self, n: usize) -> Result<&[u8], String> { if self.pos + n > self.data.len() { return Err("Unexpected end of BIN file".into()); } let s = &self.data[self.pos..self.pos+n]; self.pos += n; Ok(s) }
    fn i32(&mut self) -> Result<i32, String> { Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap())) }
    fn i64(&mut self) -> Result<i64, String> { Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap())) }
    fn f64(&mut self) -> Result<f64, String> { Ok(f64::from_be_bytes(self.take(8)?.try_into().unwrap())) }
    fn boolean(&mut self) -> Result<bool, String> { Ok(self.take(1)?[0] != 0) }
    fn string(&mut self) -> Result<String, String> { let n = self.i32()?; if n < 0 || n > 16 * 1024 * 1024 { return Err("Invalid BIN string length".into()); } String::from_utf8(self.take(n as usize)?.to_vec()).map_err(|e| e.to_string()) }
}
fn load_binary(path: &Path) -> Result<SongData, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?; let mut c = BinCursor { data, pos: 0 };
    let magic = c.string()?; if !["FNFEBIN1", "FNFEBIN2", "FNFEBIN3"].contains(&magic.as_str()) { return Err(format!("Unknown BIN chart format: {magic}")); }
    let mut s = SongData::default(); s.song = c.string()?; s.bpm = c.f64()?.max(1.0); s.needs_voices = c.boolean()?; s.player1 = c.string()?; s.player2 = c.string()?;
    if magic == "FNFEBIN2" || magic == "FNFEBIN3" { s.gf_version = c.string()?; }
    s.speed = c.f64()?.max(0.1); if magic == "FNFEBIN3" { s.mania = (c.i32()?.max(2) as usize).clamp(2, 64); }
    s.notes.clear(); let step_ms = 15_000.0 / s.bpm; let mut start_ms = 0.0;
    while c.pos < c.data.len() {
        let length = match c.i32() { Ok(v) => v, Err(_) => break };
        let must_hit = c.boolean()?; let count = c.i64()?;
        if length == 0 && count == 0 { break; }
        if count < 0 || count > 100_000_000 { return Err("Invalid note count in BIN section".into()); }
        let mut sec = Section { length_in_steps: length.max(1) as usize, must_hit_section: must_hit, notes: Vec::new() };
        for _ in 0..count {
            let global_time = c.f64()?; let lane = c.i32()?; let sustain = c.f64()?;
            if lane >= 0 && (lane as usize) < s.mania { sec.notes.push(Note { time_ms: (global_time - start_ms).max(0.0), lane: lane as usize, sustain_ms: sustain.max(0.0) }); }
        }
        start_ms += sec.length_in_steps as f64 * step_ms; s.notes.push(sec);
    }
    if s.notes.is_empty() { s.notes.push(Section::default()); } Ok(s)
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]).with_min_inner_size([950.0, 640.0]), ..Default::default() };
    eframe::run_native("Rust Editor", options, Box::new(|cc| Ok(Box::new(ChartEditor::new(&cc.egui_ctx)))))
}
