// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, process::Stdio, io::Read, sync::{Mutex, Arc}, thread::JoinHandle, fs, env::args};

use egui::{CentralPanel, Color32, Key, Slider};
use egui_video::{AudioDevice, Player, PlayerState};
use regex::Regex;
use rfd::FileDialog;

fn format_ms(ms: i64) -> String {
  let h = ms / 3600000;
  let m = ms / 60000 % 60;
  let s = ms / 1000 % 60;
  let ms = ms % 1000;
  format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

struct SnipApp {
  audio_device: AudioDevice,
  file_path: PathBuf,
  player: Option<Player>,
  start: Option<i64>,
  end: Option<i64>,
  audio_merge: u8,

  in_progress: Arc<Mutex<bool>>,
  progress: Arc<Mutex<f64>>,
  ffmpeg_handle: Option<JoinHandle<()>>,
}

impl SnipApp {
  pub fn snip(&mut self) {
    if let Some(new) = FileDialog::new()
      .add_filter("Video", &["mp4"])
      .set_file_name("video.mp4")
      .save_file()
    {
      if new.exists() {
        fs::remove_file(&new).unwrap();
      }

      let mut args = vec![
        "-i".to_owned(), self.file_path.to_str().unwrap().to_string(),
        "-c:v".to_owned(), "libx264".to_owned(),
        "-filter_complex".to_owned(), format!("amerge=inputs={}", self.audio_merge),
      ];
      if let Some(start) = self.start {
        args.push("-ss".to_owned());
        args.push(format_ms(start));
      }
      if let Some(end) = self.end {
        args.push("-to".to_owned());
        args.push(format_ms(end));
      }
      args.push(new.to_str().unwrap().to_string());

      let in_progress = self.in_progress.clone();
      let progress = self.progress.clone();

      let duration = (self.end.unwrap_or(self.player.as_ref().unwrap().duration_ms) - self.start.unwrap_or(0)) as f64;

      let handle = std::thread::spawn(move || {
        let mut ffmpeg = std::process::Command::new("ffmpeg")
          .args(args)
          .stdin(Stdio::null())
          .stdout(Stdio::null())
          .stderr(Stdio::piped())
          .spawn()
          .unwrap();

        let re = Regex::new(r"frame=.+time=(\d+):(\d+):(\d+).(\d+)").unwrap();

        {
          *in_progress.lock().unwrap() = true;
        }
        if let Some(mut stderr) = ffmpeg.stderr.take() {
          let mut a = [0u8; 256];
          while let Ok(n) = stderr.read(&mut a) {
            if n == 0 {
              break
            } else {
              let s = String::from_utf8(a.to_vec()).unwrap();
              if let Some(caps) = re.captures(&s) {
                let processed = {
                  let h = caps.get(1).unwrap().as_str().parse::<i64>().unwrap();
                  let m = caps.get(2).unwrap().as_str().parse::<i64>().unwrap();
                  let s = caps.get(3).unwrap().as_str().parse::<i64>().unwrap();
                  let ms = caps.get(4).unwrap().as_str().parse::<i64>().unwrap() * 10;
                  h * 3600000 + m * 60000 + s * 1000 + ms
                } as f64;
                *progress.lock().unwrap() = processed / duration;
              }
            }
          }
        }
        *in_progress.lock().unwrap() = false;
      });
      self.ffmpeg_handle = Some(handle);
    }
  }

  fn new(path: PathBuf) -> Self {
    Self {
      audio_device: AudioDevice::new().unwrap(),
      file_path: path,
      player: None,
      start: None,
      end: None,
      audio_merge: 1,

      in_progress: Arc::new(Mutex::new(false)),
      progress: Arc::new(Mutex::new(0.)),
      ffmpeg_handle: None,
    }
  }
}

impl eframe::App for SnipApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    CentralPanel::default().show(ctx, |ui| {
      ui.heading("Snip");
      if self.player.is_none() {
        match Player::new(
          ctx,
          &self.file_path.to_str().unwrap().to_string())
            .and_then(|p| p.with_audio(&mut self.audio_device)
        ) {
          Ok(mut player) => {
            player.looping = false;
            self.player = Some(player);
          },
          Err(_) => panic!("failed to create player"),
        }
      }

      if let Some(player) = self.player.as_mut() {
        // Player
        ui.allocate_ui(player.size * 0.5, |ui| {
          player.ui(ui, player.size * 0.5);
        });
        // Controls
        ui.vertical_centered_justified(|ui| {
          ui.horizontal(|ui| {
            if ui.button("Start").clicked()
              || ui.input(|i| i.key_pressed(Key::S)) {
                self.start = Some(player.elapsed_ms());
            }
            if let Some(start) = self.start {
              ui.label(format_ms(start));
            } else {
              ui.label("not set");
            }
          });
          ui.horizontal(|ui| {
            if ui.button("End").clicked()
              || ui.input(|i| i.key_pressed(Key::E)) {
                self.end = Some(player.elapsed_ms());
            }
            if let Some(end) = self.end {
              ui.label(format_ms(end));
            } else {
              ui.label("not set");
            }
          });
          match (self.start, self.end) {
            (Some(start), Some(end)) => {
              if start > end {
                ui.colored_label(Color32::RED, "start > end");
              }
            },
            _ => {}
          }
        });
        {
          if ui.button("Cycle audio channel").clicked()
            || ui.input(|i| i.key_pressed(Key::A)) {
              player.cycle_audio_stream();
          }
          let label = ui.label("Merge audio channels:");
          ui.add(Slider::new(&mut self.audio_merge, 1..=4)).labelled_by(label.id);
        }
        if *self.in_progress.lock().unwrap() {
          let progress = *self.progress.lock().unwrap();
          ui.label(format!("Progress: {:.2}%", progress * 100.));
        }
        // Keybinds
        if ui.input(|i| i.key_pressed(Key::Space)) {
          match player.player_state.get() {
            PlayerState::Playing => player.pause(),
            PlayerState::Paused => player.resume(),
            PlayerState::EndOfFile => {
              player.seek(0.);
              player.start();
            },
            PlayerState::Stopped => player.start(),
            _ => {},
          }
        }
        let step = if ui.input(|i| i.modifiers.shift) { 1000 } else { 5000 };
        if ui.input(|i| i.key_pressed(Key::ArrowLeft)) {
          let s = ((player.elapsed_ms() - step) as f32 / player.duration_ms as f32).max(0.);
          println!("{}", s);
          player.seek(((player.elapsed_ms() - step) as f32 / player.duration_ms as f32).max(0.));
        }
        if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
          player.seek(((player.elapsed_ms() + step) as f32 / player.duration_ms as f32).min(1.));
        }
        // Snip
        if ui.input(|i| i.key_pressed(Key::Enter)) {
          self.snip();
        }
      }
    });
  }
}

fn main() {
  let options = eframe::NativeOptions {
    initial_window_size: Some(egui::Vec2 { x: 1280., y: 720. }),
    ..Default::default()
  };
  let path = match args().into_iter().skip(1).next() {
    Some(path) => PathBuf::from(path),
    None => FileDialog::new()
      .set_title("Open video")
      .pick_file()
      .expect("no video file provided"),
  };
  let _ = eframe::run_native("snip", options, Box::new(|_| {
    Box::new(SnipApp::new(path))
  }));
}