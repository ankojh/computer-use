//! Run folder: `run.json`, `events.jsonl`, `frames/`, `summary.json` (HANDOFF.md §13).

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use anyhow::Context;
use serde::Serialize;
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::types::EncodedImage;

pub struct RunRecorder {
    dir: PathBuf,
    started: Instant,
    inner: Mutex<Inner>,
}

struct Inner {
    events: BufWriter<File>,
    frames: u32,
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
}

fn create_private_dir(path: &Path) -> anyhow::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .with_context(|| format!("creating {}", path.display()))
}

fn create_private_file(path: &Path) -> anyhow::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))
}

impl RunRecorder {
    /// Creates `<runs_dir>/<YYYYMMDD-HHMMSS>-<shortid>/` (mode 0700). Screenshots are sensitive.
    pub fn create(runs_dir: &Path) -> anyhow::Result<Self> {
        let stamp = now()
            .format(time::macros::format_description!(
                "[year][month][day]-[hour][minute][second]"
            ))
            .expect("static format");
        let short = &uuid::Uuid::new_v4().simple().to_string()[..8];
        let dir = runs_dir.join(format!("{stamp}-{short}"));
        create_private_dir(runs_dir)?;
        create_private_dir(&dir)?;
        create_private_dir(&dir.join("frames"))?;
        let events = BufWriter::new(create_private_file(&dir.join("events.jsonl"))?);
        Ok(RunRecorder {
            dir,
            started: Instant::now(),
            inner: Mutex::new(Inner { events, frames: 0 }),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Writes a pretty JSON file into the run folder (`run.json`, `summary.json`).
    pub fn write_json<T: Serialize>(&self, name: &str, value: &T) -> anyhow::Result<()> {
        let mut f = create_private_file(&self.dir.join(name))?;
        serde_json::to_writer_pretty(&mut f, value)?;
        f.write_all(b"\n")?;
        Ok(())
    }

    /// Appends one event. `data` must be a JSON object; its fields are merged into the event.
    /// Logging never fails the run: errors are reported once to stderr.
    pub fn event(&self, kind: &str, data: Value) {
        let mut ev = json!({
            "t_ms": self.elapsed_ms(),
            "ts": now().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            "kind": kind,
        });
        if let (Value::Object(ev), Value::Object(data)) = (&mut ev, data) {
            ev.extend(data);
        }
        let mut inner = self.inner.lock().unwrap();
        let res = serde_json::to_writer(&mut inner.events, &ev)
            .map_err(anyhow::Error::from)
            .and_then(|_| inner.events.write_all(b"\n").map_err(Into::into))
            .and_then(|_| inner.events.flush().map_err(Into::into));
        if let Err(e) = res {
            tracing::warn!("failed to write event: {e:#}");
        }
    }

    /// Saves an image as `frames/NNNN.<ext>` and returns the run-relative path.
    pub fn save_frame(&self, image: &EncodedImage) -> anyhow::Result<String> {
        let n = {
            let mut inner = self.inner.lock().unwrap();
            inner.frames += 1;
            inner.frames
        };
        let rel = format!("frames/{n:04}.{}", image.format.extension());
        let mut f = create_private_file(&self.dir.join(&rel))?;
        f.write_all(&image.bytes)?;
        Ok(rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ImageFormat;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;

    #[test]
    fn creates_private_run_folder() {
        let tmp = std::env::temp_dir().join(format!("cu-rec-{}", uuid::Uuid::new_v4().simple()));
        let rec = RunRecorder::create(&tmp).unwrap();
        let mode = fs::metadata(rec.dir()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);

        rec.event("observation", json!({"frame": "frames/0001.jpg"}));
        rec.event("done", json!({}));
        let img = EncodedImage {
            bytes: Arc::new(vec![1, 2, 3]),
            format: ImageFormat::Png,
            width: 1,
            height: 1,
        };
        assert_eq!(rec.save_frame(&img).unwrap(), "frames/0001.png");
        assert_eq!(rec.save_frame(&img).unwrap(), "frames/0002.png");
        rec.write_json("summary.json", &json!({"ok": true}))
            .unwrap();

        let events = fs::read_to_string(rec.dir().join("events.jsonl")).unwrap();
        let lines: Vec<Value> = events
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["kind"], "observation");
        assert_eq!(lines[0]["frame"], "frames/0001.jpg");
        fs::remove_dir_all(&tmp).unwrap();
    }
}
