//! Native chat document operations and persistent message logs.
use super::*;
use std::io::{Read, Seek, SeekFrom, Write};

pub(super) struct OperationDialog {
    window: WindowId,
    operation: String,
    document: String,
}


pub(super) struct LogWindow {
    window: WindowId,
    follow: bool,
    text: String,
    offset: u64,
    pending: String,
    follow_age: f32,
}

impl LogWindow {
    fn refresh(&mut self, gui: &mut Gui, path: &Path, initial: bool) -> Result<()> {
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let length = file.metadata()?.len();
        if !initial && length == self.offset { return Ok(()); }
        let follow_bottom = initial || gui.view_rect(self.window,"log").zip(gui.view_rect(self.window,"log_scroll"))
            .is_none_or(|(content,viewport)| content.b-content.t-(viewport.b-viewport.t)-gui.scroll_offset(self.window,"log_scroll") < 4.0);
        let start = if initial { length.saturating_sub(0x40000) } else if length < self.offset { 0 } else { self.offset };
        if initial || length < self.offset { self.text.clear(); self.pending.clear(); }
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        self.offset = start + bytes.len() as u64;
        let content = if initial && start != 0 {
            bytes.iter().position(|byte| *byte == b'\n').map_or(&[][..], |end| &bytes[end+1..])
        } else { &bytes };
        self.pending.push_str(&String::from_utf8_lossy(content));
        let mut consumed = 0;
        for record in self.pending.split_inclusive('\n') {
            if !record.ends_with('\n') { break; }
            self.text.push_str(&parse_log_record(record.trim_end_matches(['\r','\n']))?);
            consumed += record.len();
        }
        self.pending.drain(..consumed);
        gui.set_text(self.window, "log", &self.text);
        if follow_bottom { gui.scroll_to_bottom(self.window, "log_scroll"); }
        Ok(())
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn event_window(event: &Event) -> Option<WindowId> {
    match event {
        Event::Clicked { window, .. } | Event::TextChanged { window, .. }
        | Event::EnterPressed { window, .. } | Event::Escape { window }
        | Event::CloseRequested { window } | Event::WindowFrame { window }
        | Event::LinkClicked { window, .. } => Some(*window),
        _ => None,
    }
}
fn log_path(prefs: &Path, document: &str) -> PathBuf {
    prefs.join("Chat/Windows").join(document).join("Log.txt")
}

fn open_log(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::OpenOptions::new().create(true).append(true).open(path)
}



fn log_record(group: u64, group_name: &str, sender: &str, epoch: u32, body: &str) -> String {
    let mut record = String::with_capacity(group_name.len()+sender.len()+body.len()+64);
    let _ = write!(record,"[\"#{group:016x}#\",");
    for field in [group_name,sender] {
        record.push('"');
        for character in field.chars() {
            match character {
                '\\' => record.push_str("\\\\"),
                '"' => record.push_str("\\\""),
                '\n' => record.push_str("\\n"),
                _ => record.push(character),
            }
        }
        record.push_str("\",");
    }
    let _ = write!(record,"{epoch}]");
    for character in body.chars() {
        match character { '\r' => {}, '\n' => record.push_str("<br>"), _ => record.push(character) }
    }
    record.push('\n');
    record
}

fn log_html(group_name: &str, sender: &str, body: &str) -> String {
    let mut html = String::with_capacity(group_name.len()+sender.len()+body.len()+40);
    let _ = write!(html,"<div indent=wrapped>[{group_name}] ");
    if !sender.is_empty() { let _ = write!(html,"{sender}: "); }
    html.push_str(body);
    html.push_str("</div>");
    html
}

fn parse_log_record(record: &str) -> Result<String> {
    let mut remaining = record.strip_prefix('[').ok_or_else(|| anyhow::anyhow!("missing chat log header"))?;
    let mut fields: [String;3] = std::array::from_fn(|_| String::new());
    for field in &mut fields {
        remaining = remaining.strip_prefix('"').ok_or_else(|| anyhow::anyhow!("missing quoted chat log field"))?;
        let mut escaped = false;
        let mut end = None;
        for (index,character) in remaining.char_indices() {
            if escaped {
                field.push(if character == 'n' { '\n' } else { character });
                escaped = false;
            } else if character == '\\' { escaped = true; }
            else if character == '"' { end = Some(index+1); break; }
            else { field.push(character); }
        }
        remaining = remaining.get(end.ok_or_else(|| anyhow::anyhow!("unterminated chat log field"))?..)
            .and_then(|remaining| remaining.strip_prefix(',')).ok_or_else(|| anyhow::anyhow!("missing chat log field separator"))?;
    }
    let (epoch,body) = remaining.split_once(']').ok_or_else(|| anyhow::anyhow!("missing chat log body"))?;
    epoch.parse::<u32>()?;
    // Log records always write the group as `#{:016x}#` (`log_record`); reject anything else, including bare hex.
    let group = &fields[0];
    anyhow::ensure!(group.len() == 18 && group.starts_with('#') && group.ends_with('#') && group[1..17].bytes().all(|b| b.is_ascii_hexdigit()), "invalid chat log group");
    Ok(log_html(&fields[1], &fields[2], body))
}
impl ChatWindows {
    pub(super) fn config_ops_owns(&self, event: &Event) -> bool {
        event_window(event).is_some_and(|window| self.config_ops.as_ref().is_some_and(|dialog| dialog.window == window) || self.config_logs.values().any(|log| log.window == window))
    }

    fn operation_label(&self, category: u32, key: &str) -> String {
        self.text.as_ref().and_then(|db| db.by_key(category, key)).filter(|s| !s.is_empty()).unwrap_or_else(|| key.to_owned())
    }

    fn operation_text(&self, key: &str, name: &str) -> String {
        super::super::log::ldb_format(&self.label(key), &[super::super::log::Arg::S(name.to_owned())])
    }

    fn open_operation_dialog(&mut self, gui: &mut Gui, document: usize, operation: &str) -> Result<()> {
        if let Some(dialog) = self.config_ops.take() { gui.close_window(dialog.window); }
        let cfg = self.wins.get(document).map(|win| &win.cfg);
        let clear = operation == "ClearLog";
        let remove = matches!(operation, "RemoveWindow" | "DeleteWindow") || clear;
        let protected = !clear && remove && cfg.is_some_and(|cfg| cfg.startup);
        let title = if clear { "Warning:".to_owned() } else { self.label(if remove { "Warning" } else { "EnterWindowName" }) };
        let body = if clear { self.operation_label(10000, "PermaDelChatLog") }
            else if protected { self.label("NotPossible2DeleteDefaultWnd") }
            else if remove { self.operation_text("Sure2DeleteWindow", cfg.map_or("", |cfg| cfg.name.as_str())) }
            else { self.label("EnterNameOfNewWindow") };
        let initial = match operation {
            "RenameWindow" => cfg.map_or(String::new(), |cfg| cfg.name.clone()),
            "CloneWindow" => self.operation_text("CopyOf", cfg.map_or("", |cfg| cfg.name.as_str())),
            _ => String::new(),
        };
        let mut source = if remove {
            format!("<root><View view_layout=\"vertical\"><TextView name=\"body\" feature_flags=\"TVF_MULTILINE|TVF_WORD_WRAP\" value=\"{}\" layout_borders=\"Rect(15,5,15,20)\"/>", escape(&body))
        } else {
            format!("<root><View view_layout=\"vertical\"><View layout_borders=\"Rect(15,5,15,20)\"><TextView name=\"body\" value=\"{}\" layout_borders=\"Rect(0,0,0,10)\"/><TextInputView name=\"input\" min_size=\"Point(150,-1)\" max_size=\"Point(150,-1)\"/></View>", escape(&body))
        };
        source += "<View view_layout=\"horizontal\" layout_borders=\"Rect(0,0,0,5)\"><HLayoutSpacer/>";
        let first = self.operation_label(10000, if remove && !protected { "MsgBox_Yes" } else { "MsgBox_OK" });
        source += &format!("<Button name=\"ok\" label=\"{}\" layout_borders=\"Rect(8,0,8,0)\"/>", escape(&first));
        if !protected {
            let second = self.operation_label(10000, if remove { "MsgBox_No" } else { "MsgBox_Cancel" });
            source += &format!("<Button name=\"cancel\" label=\"{}\" layout_borders=\"Rect(8,0,8,0)\"/>", escape(&second));
        } else { source += "<HLayoutSpacer/>"; }
        source += "</View></View></root>";
        let window = gui.open_framed_window_xml(&title, &source, (0,0), WindowSize::Preferred)?;
        gui.set_default_button(window, "ok");
        if !remove { gui.set_text(window, "input", &initial); gui.focus(window, "input"); }
        gui.relayout_window(window);
        let (width,height) = gui.outer_size(window);
        gui.set_window_pos(window, ((self.screen.0 as i32-width as i32)/2,(self.screen.1 as i32-height as i32)/2));
        self.config_ops = Some(OperationDialog { window, operation: if protected { "ProtectedWindow" } else { operation }.to_owned(), document: cfg.map_or(String::new(), |cfg| cfg.window_name.clone()) });
        Ok(())
    }

    pub(super) fn config_operation(&mut self, gui: &mut Gui, document: usize, operation: &str) {
        let result = match operation {
            "NewWindow" | "RenameWindow" | "CloneWindow" | "RemoveWindow" | "DeleteWindow" => self.open_operation_dialog(gui, document, operation),
            "LogMessages" => {
                if let Some(win) = self.wins.get_mut(document) {
                    win.cfg.logged ^= true;
                    if !win.cfg.logged { self.config_log_files.remove(&win.cfg.window_name); }
                    self.dirty = true;
                }
                Ok(())
            }
            "ShowMessageLog" => self.toggle_message_log(gui, document),
            _ => return,
        };
        if let Err(error) = result { eprintln!("chat: {operation} failed: {error:#}"); }
    }

    fn create_document(&mut self, gui: &mut Gui, source: Option<usize>, name: &str) -> Result<()> {
        let mut number = 1;
        let document = loop {
            let document = format!("Window{number}");
            if !self.wins.iter().any(|win| win.cfg.window_name == document)
                && !self.prefs.as_ref().is_some_and(|prefs| prefs.join("Chat/Windows").join(&document).exists()) {
                break document;
            }
            number += 1;
        };
        let directory = self.prefs.as_ref().map(|prefs| prefs.join("Chat/Windows").join(&document));
        if let Some(directory) = &directory { std::fs::create_dir_all(directory)?; }
        let mut cfg = source.map_or_else(|| Cfg::blank(name, &document), |source| self.wins[source].cfg.clone());
        cfg.name = name.to_owned();
        cfg.window_name = document.clone();
        cfg.default_window = false;
        cfg.startup = false;
        cfg.open = source.is_none();
        if source.is_none() { cfg.visual_mode = 0; }
        cfg.tab_index = 0;
        cfg.template = false;
        let outer = place(cfg.frame, self.screen);
        let index = self.add_doc(cfg);
        if let Err(error) = self.add_frame(gui, vec![index], outer) {
            self.wins.pop();
            if let Some(directory) = directory { let _ = std::fs::remove_dir(directory); }
            return Err(error);
        }
        self.config.selected = document;
        self.dirty = true;
        Ok(())
    }

    fn remove_document(&mut self, gui: &mut Gui, document: usize) -> Result<()> {
        if self.wins[document].cfg.startup { return Ok(()); }
        let name = self.wins[document].cfg.window_name.clone();
        self.config_log_files.remove(&name);
        // Remove the archive first: a failed deletion must not resurrect an apparently removed document at next login.
        if let Some(prefs) = &self.prefs {
            let path = prefs.join("Chat/Windows").join(&name);
            if path.exists() {
                // Native 10093f15 unlinks these two files, never recursively removes unrelated history.
                for file in ["Config.xml","Log.txt"] {
                    match std::fs::remove_file(path.join(file)) {
                        Ok(()) => {},
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                        Err(error) => return Err(error.into()),
                    }
                }
                if std::fs::read_dir(&path)?.next().is_none() { std::fs::remove_dir(path)?; }
            }
        }
        if let Some(log) = self.config_logs.remove(&name) { gui.close_window(log.window); }
        let frame = self.wins[document].frame;
        self.frames[frame].docs.retain(|index| *index != document);
        self.wins.remove(document);
        for frame in &mut self.frames {
            for index in &mut frame.docs { if *index > document { *index -= 1; } }
        }
        if self.frames[frame].docs.is_empty() {
            gui.close_window(self.frames[frame].id);
            self.frames.remove(frame);
        } else {
            self.frames[frame].sel = self.frames[frame].sel.min(self.frames[frame].docs.len()-1);
            self.rebuild_frame(gui, frame);
        }
        self.reindex();
        self.config.selected = self.wins.first().map_or(String::new(), |win| win.cfg.window_name.clone());
        if self.last_active == name { self.last_active.clone_from(&self.config.selected); }
        self.dirty = true;
        Ok(())
    }

    pub(super) fn config_ops_event(&mut self, gui: &mut Gui, event: &Event) -> bool {
        if !self.config_ops_owns(event) { return false; }
        let window = event_window(event).unwrap();
        if self.config_logs.values().any(|log| log.window == window) {
            self.message_log_event(gui, window, event);
            return true;
        }
        let accept = matches!(event, Event::EnterPressed { .. }) || matches!(event, Event::Clicked { view, .. } if view == "ok");
        let cancel = matches!(event, Event::Escape { .. } | Event::CloseRequested { .. }) || matches!(event, Event::Clicked { view, .. } if view == "cancel");
        if !accept && !cancel { return true; }
        let Some(dialog) = self.config_ops.take() else { return true; };
        let name = gui.text(window, "input");
        let document = self.wins.iter().position(|win| win.cfg.window_name == dialog.document);
        if accept && matches!(dialog.operation.as_str(), "NewWindow" | "CloneWindow" | "RenameWindow") &&
            self.wins.iter().enumerate().any(|(index,win)| win.cfg.name == name && !(dialog.operation == "RenameWindow" && Some(index) == document)) {
            gui.set_text(window, "body", &self.label("WindowWithNameExists"));
            self.config_ops = Some(dialog);
            return true;
        }
        gui.close_window(window);
        if cancel { return true; }
        let result = match dialog.operation.as_str() {
            "NewWindow" => self.create_document(gui, None, &name),
            "CloneWindow" => document.map_or(Ok(()), |document| self.create_document(gui, Some(document), &name)),
            "RenameWindow" => {
                if let Some(document) = document {
                    self.wins[document].cfg.name = name;
                    self.sync_decor(gui);
                    if let Some(log) = self.config_logs.get(&self.wins[document].cfg.window_name) {
                        let title = self.operation_text("ChatLogViewWindowTitle",&self.wins[document].cfg.name);
                        gui.set_window_tabs(log.window,&[title],0);
                    }
                    self.dirty = true;
                }
                Ok(())
            }
            "RemoveWindow" | "DeleteWindow" => document.map_or(Ok(()), |document| self.remove_document(gui, document)),
            "ClearLog" => self.clear_message_log(gui, &dialog.document),
            _ => Ok(()),
        };
        if let Err(error) = result { eprintln!("chat: {} failed: {error:#}", dialog.operation); }
        true
    }

    fn toggle_message_log(&mut self, gui: &mut Gui, document: usize) -> Result<()> {
        let Some(cfg) = self.wins.get(document).map(|win| &win.cfg) else { return Ok(()); };
        if let Some(window) = self.config_logs.get(&cfg.window_name).map(|log| log.window) {
            self.message_log_event(gui,window,&Event::CloseRequested { window });
            return Ok(());
        }
        let title = self.operation_text("ChatLogViewWindowTitle", &cfg.name);
        let path = self.prefs.as_ref().map(|prefs| log_path(prefs, &cfg.window_name));
        let mut source = format!("<root><View min_size=\"Point(639,99)\"><BorderView layout_borders=\"Rect(5,3,5,0)\"><TextView name=\"path\" value=\"{}\" min_size=\"Point(149,-1)\"/></BorderView><BorderView layout_borders=\"Rect(5,5,5,5)\"><ScrollView name=\"log_scroll\"><TextView name=\"log\" feature_flags=\"TVF_MULTILINE|TVF_WORD_WRAP|TVF_ALLOW_TEXT_SELECTION|TVF_ACCEPT_MOUSE_INPUT\" min_size=\"Point(149,99)\" max_size=\"Point(16000,16000)\"/></ScrollView></BorderView><View view_layout=\"horizontal\" layout_borders=\"Rect(5,3,5,3)\"><HLayoutSpacer/>", escape(&path.as_ref().map_or(String::new(), |path| path.display().to_string())));
        for key in ["Refresh","Follow","Clear"] {
            source += &format!("<Button name=\"{key}\" label=\"{}\" layout_borders=\"Rect(5,0,0,0)\"/>", escape(&self.label(&format!("ChatLogView_Button_{key}"))));
        }
        source += "</View></View></root>";
        let (position,size) = cfg.log_frame.map_or(((0,0),WindowSize::Preferred), |frame| {
            let (x,y,width,height) = place(Some(frame),self.screen);
            ((x,y),WindowSize::Fixed(width,height))
        });
        let window = gui.open_tabbed_window_xml("ChatLog",&title,&source,position,size)?;
        if cfg.log_frame.is_none() {
            let (width,height) = gui.outer_size(window);
            gui.set_window_pos(window,((self.screen.0 as i32-width as i32)/2,(self.screen.1 as i32-height as i32)/2));
        }
        let mut log = LogWindow { window, follow: false, text: String::new(), offset: 0, pending: String::new(), follow_age: 0.0 };
        if let Some(path) = path {
            if let Err(error) = log.refresh(gui,&path,true) { gui.close_window(window); return Err(error); }
        }
        self.config_logs.insert(cfg.window_name.clone(),log);
        Ok(())
    }

    fn clear_message_log(&mut self, gui: &mut Gui, document: &str) -> Result<()> {
        self.config_log_files.remove(document);
        if let Some(prefs) = &self.prefs { std::fs::write(log_path(prefs,document), [])?; }
        if let Some(log) = self.config_logs.get_mut(document) {
            log.text.clear(); log.pending.clear(); log.offset = 0;
            gui.set_text(log.window,"log","");
        }
        Ok(())
    }

    fn message_log_event(&mut self, gui: &mut Gui, window: WindowId, event: &Event) {
        let Some(document) = self.config_logs.iter().find(|(_,log)| log.window == window).map(|(document,_)| document.clone()) else { return; };
        let index = self.wins.iter().position(|win| win.cfg.window_name == document);
        match event {
            Event::WindowFrame { .. } | Event::CloseRequested { .. } | Event::Escape { .. } => {
                if let (Some(index),Some((x,y,width,height))) = (index,gui.window_outer_frame(window)) {
                    self.wins[index].cfg.log_frame = Some([x as f32,y as f32,(x+width as i32-1) as f32,(y+height as i32-1) as f32]);
                    self.dirty = true;
                }
                if !matches!(event,Event::WindowFrame { .. }) {
                    self.config_logs.remove(&document);
                    gui.close_window(window);
                }
            }
            Event::Clicked { view, .. } if view == "Refresh" => {
                if let (Some(prefs),Some(log)) = (&self.prefs,self.config_logs.get_mut(&document)) {
                    if let Err(error) = log.refresh(gui,&log_path(prefs,&document),false) { eprintln!("chat: refreshing log failed: {error:#}"); }
                }
            }
            Event::Clicked { view, .. } if view == "Follow" => {
                if let Some(log) = self.config_logs.get_mut(&document) {
                    log.follow ^= true;
                    log.follow_age = 0.0;
                    gui.set_button_pressed(window,"Follow",log.follow);
                    if log.follow {
                        if let Some(prefs) = &self.prefs {
                            if let Err(error) = log.refresh(gui,&log_path(prefs,&document),false) { eprintln!("chat: following log failed: {error:#}"); }
                        }
                    }
                }
            }
            Event::Clicked { view, .. } if view == "Clear" => {
                if let Some(index) = index {
                    if let Err(error) = self.open_operation_dialog(gui,index,"ClearLog") { eprintln!("chat: opening clear log dialog failed: {error:#}"); }
                }
            }
            _ => {}
        }
    }

    pub(super) fn config_ops_update(&mut self, gui: &mut Gui, dt: f32) {
        let Some(prefs) = &self.prefs else { return; };
        for (document,log) in &mut self.config_logs {
            if log.follow {
                log.follow_age += dt;
                if log.follow_age < 1.0 { continue; }
                log.follow_age %= 1.0; // GUI.dll DAT_1026bb70: 1,000,000 microseconds.
                if let Err(error) = log.refresh(gui,&log_path(prefs,document),false) { eprintln!("chat: following log failed: {error:#}"); }
            }
        }
    }

    pub(super) fn config_ops_close_all(&mut self, gui: &mut Gui) {
        if let Some(dialog) = self.config_ops.take() { gui.close_window(dialog.window); }
        let windows: Vec<_> = self.config_logs.values().map(|log| log.window).collect();
        for window in windows { self.message_log_event(gui,window,&Event::CloseRequested { window }); }
        self.config_log_files.clear();
    }

    pub(super) fn config_log_message(&mut self, _gui: &mut Gui, document: usize, message: &ChatMsg, group_name: &str) {
        let cfg = &self.wins[document].cfg;
        if !cfg.logged { return; }
        let Some(prefs) = &self.prefs else { return; };
        if !self.config_log_files.contains_key(&cfg.window_name) {
            let path = log_path(prefs, &cfg.window_name);
            let opened = open_log(&path);
            match opened {
                Ok(file) => { self.config_log_files.insert(cfg.window_name.clone(),file); }
                Err(error) => { eprintln!("chat: writing {} failed: {error}",path.display()); return; }
            }
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_secs() as u32);
        let record = log_record(message.group, group_name, &message.from_name, now, &message.text);
        if let Some(file) = self.config_log_files.get_mut(&cfg.window_name) {
            if let Err(error) = file.write_all(record.as_bytes()).and_then(|_| file.flush()) { eprintln!("chat: writing log failed: {error}"); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn message_archive_appends_without_overwriting() {
        let directory = std::env::temp_dir().join(format!("aomac-chat-log-{}", std::process::id()));
        let path = log_path(&directory, "Window7");
        let _ = std::fs::remove_dir_all(&directory);
        let first = log_record(G_VICINITY,"Vicinity","Bob",17,"first\nsecond\r");
        let second = log_record(G_SYSTEM,"System","",18,"third");
        open_log(&path).unwrap().write_all(first.as_bytes()).unwrap();
        open_log(&path).unwrap().write_all(second.as_bytes()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first+&second);
        assert_eq!(parse_log_record(second.trim_end()).unwrap(),"<div indent=wrapped>[System] third</div>");
        assert!(parse_log_record("[bad]garbage").is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_log_header_and_body_round_trip() {
        let record = log_record(G_VICINITY,"OOC \"Help\"","Bob",23,"hello\r\nworld");
        assert_eq!(record,"[\"#0000000040000002#\",\"OOC \\\"Help\\\"\",\"Bob\",23]hello<br>world\n");
        assert_eq!(parse_log_record(record.trim_end()).unwrap(),"<div indent=wrapped>[OOC \"Help\"] Bob: hello<br>world</div>");
        for bad in ["", "[\"a\",\"b\",\"c\",x]body", "[\"bad\",\"group\",\"sender\",12]body", "[\"unterminated"] {
            assert!(parse_log_record(bad).is_err(),"{bad}");
        }
    }

    #[test]
    fn native_document_dialogs_persist_and_remove_archives() {
        let client = ao_gui::client_dir();
        if !client.join("cd_image/gui/Default").is_dir() { return; }
        let directory = std::env::temp_dir().join(format!("aomac-chat-ops-{}",std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let mut gui = Gui::new(&client,None).unwrap();
        let mut windows = ChatWindows::new_with_prefs(&mut gui,(1280,800),Some(directory.clone())).unwrap();
        windows.config_operation(&mut gui,0,"NewWindow");
        let dialog = windows.config_ops.as_ref().unwrap().window;
        gui.set_text(dialog,"input","A & B's");
        assert!(windows.config_ops_event(&mut gui,&Event::EnterPressed { window: dialog,view: "input".into() }));
        let document = windows.wins.iter().position(|win| win.cfg.name == "A & B's").unwrap();
        assert_eq!(windows.wins[document].cfg.visual_mode,0);
        windows.wins[document].cfg.set = vec![G_VICINITY];
        windows.config_operation(&mut gui,document,"LogMessages");
        let message = ChatMsg { group:G_VICINITY,from_name:"Bob".into(),text:"persisted".into(),..Default::default() };
        windows.push_msg(&mut gui,&message);
        let document_name = windows.wins[document].cfg.window_name.clone();
        let log_path = log_path(&directory,&document_name);
        assert!(std::fs::read_to_string(&log_path).unwrap().ends_with("]persisted\n"));
        windows.config_operation(&mut gui,document,"ShowMessageLog");
        let viewer = windows.config_logs[&document_name].window;
        assert!(gui.text(viewer,"log").contains("Bob: persisted"));
        windows.config_ops_event(&mut gui,&Event::Clicked { window:viewer,view:"Follow".into(),item:None });
        windows.push_msg(&mut gui,&ChatMsg { text:"followed".into(),..message.clone() });
        windows.config_ops_update(&mut gui,1.0);
        assert!(gui.text(viewer,"log").contains("followed"));
        windows.config_ops_event(&mut gui,&Event::Clicked { window:viewer,view:"Clear".into(),item:None });
        let clear = windows.config_ops.as_ref().unwrap().window;
        windows.config_ops_event(&mut gui,&Event::Clicked { window:clear,view:"cancel".into(),item:None });
        assert!(std::fs::metadata(&log_path).unwrap().len() > 0);
        windows.config_operation(&mut gui,document,"RenameWindow");
        let rename = windows.config_ops.as_ref().unwrap().window;
        gui.set_text(rename,"input","Renamed & kept");
        windows.config_ops_event(&mut gui,&Event::EnterPressed { window:rename,view:"input".into() });
        assert_eq!(windows.wins[document].cfg.window_name,document_name);
        assert_eq!(windows.wins[document].cfg.name,"Renamed & kept");
        assert!(std::fs::metadata(&log_path).unwrap().len() > 0);
        windows.config_operation(&mut gui,document,"CloneWindow");
        let dialog = windows.config_ops.as_ref().unwrap().window;
        gui.set_text(dialog,"input","Copied");
        windows.config_ops_event(&mut gui,&Event::EnterPressed { window: dialog,view: "input".into() });
        let copied = windows.wins.iter().position(|win| win.cfg.name == "Copied").unwrap();
        assert!(!windows.wins[copied].cfg.open);
        assert_eq!(windows.wins[copied].cfg.set,vec![G_VICINITY]);
        assert!(windows.wins[copied].cfg.logged);
        let copied_name = windows.wins[copied].cfg.window_name.clone();
        windows.save().unwrap();
        let saved = read_windows(&directory);
        assert!(saved.iter().any(|cfg| cfg.name == "Renamed & kept" && cfg.logged));
        assert!(saved.iter().any(|cfg| cfg.name == "Copied" && cfg.set == vec![G_VICINITY]));
        let mut reopened_gui = Gui::new(&client,None).unwrap();
        let reopened = ChatWindows::new_with_prefs(&mut reopened_gui,(1280,800),Some(directory.clone())).unwrap();
        let clone_doc = reopened.wins.iter().position(|win| win.cfg.window_name == copied_name).unwrap();
        let source_doc = reopened.wins.iter().position(|win| win.cfg.window_name == document_name).unwrap();
        assert!(!reopened.wins[clone_doc].cfg.open);
        assert_ne!(reopened.wins[clone_doc].frame,reopened.wins[source_doc].frame,"closed clone must not appear in source tabs after reload");
        assert!(!directory.join("Chat/Windows").join(&copied_name).join("Log.txt").exists(),"clone must not copy message archives");
        windows.config_ops_event(&mut gui,&Event::Clicked { window:viewer,view:"Clear".into(),item:None });
        let clear = windows.config_ops.as_ref().unwrap().window;
        windows.config_ops_event(&mut gui,&Event::Clicked { window:clear,view:"ok".into(),item:None });
        assert_eq!(std::fs::metadata(&log_path).unwrap().len(),0);
        windows.config_operation(&mut gui,document,"RenameWindow");
        let duplicate = windows.config_ops.as_ref().unwrap().window;
        gui.set_text(duplicate,"input","Copied");
        windows.config_ops_event(&mut gui,&Event::EnterPressed { window:duplicate,view:"input".into() });
        assert!(windows.config_ops.is_some(),"duplicate name keeps the input dialog");
        windows.config_ops_event(&mut gui,&Event::Escape { window:duplicate });
        windows.wins[document].cfg.startup = true;
        windows.config_operation(&mut gui,document,"RemoveWindow");
        let protected = windows.config_ops.as_ref().unwrap().window;
        windows.config_ops_event(&mut gui,&Event::Clicked { window:protected,view:"ok".into(),item:None });
        assert!(windows.wins.iter().any(|win| win.cfg.window_name == document_name),"native startup document is protected");
        windows.config_operation(&mut gui,copied,"RemoveWindow");
        let dialog = windows.config_ops.as_ref().unwrap().window;
        windows.config_ops_event(&mut gui,&Event::Clicked { window: dialog,view:"cancel".into(),item:None });
        assert!(windows.wins.iter().any(|win| win.cfg.window_name == copied_name));
        let copied_path = directory.join("Chat/Windows").join(&copied_name);
        std::fs::write(copied_path.join("InputHistory.xml"),"retained history").unwrap();
        std::fs::write(copied_path.join("Log.txt"),"removed archive").unwrap();
        windows.config_operation(&mut gui,copied,"RemoveWindow");
        let dialog = windows.config_ops.as_ref().unwrap().window;
        windows.config_ops_event(&mut gui,&Event::Clicked { window:dialog,view:"ok".into(),item:None });
        windows.save().unwrap();
        assert!(!read_windows(&directory).iter().any(|cfg| cfg.window_name == copied_name));
        assert!(!copied_path.join("Config.xml").exists());
        assert!(!copied_path.join("Log.txt").exists());
        assert_eq!(std::fs::read_to_string(copied_path.join("InputHistory.xml")).unwrap(),"retained history");
        windows.config_ops_close_all(&mut gui);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
