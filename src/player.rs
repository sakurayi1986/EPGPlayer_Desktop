//! libmpv 播放器。mpv 自己开窗口，运行在独立线程里。
//! 窗口开着时再点播放会在同一个窗口里切换；关掉窗口后线程结束，下次重新创建。

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

use libmpv2::events::Event;
use libmpv2::{mpv_end_file_reason, Mpv};

enum Cmd {
    Load { url: String, title: String },
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub running: bool,
    pub title: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Player {
    tx: Option<Sender<Cmd>>,
    status: Arc<Mutex<Status>>,
}

impl Player {
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn play(&mut self, url: String, title: String, repaint: impl Fn() + Send + 'static) {
        let cmd = Cmd::Load { url, title };
        let cmd = match &self.tx {
            Some(tx) if self.status.lock().unwrap().running => match tx.send(cmd) {
                Ok(()) => return,
                Err(mpsc::SendError(c)) => c,
            },
            _ => cmd,
        };

        let (tx, rx) = mpsc::channel();
        tx.send(cmd).ok();
        self.tx = Some(tx);
        *self.status.lock().unwrap() = Status { running: true, ..Default::default() };
        let status = self.status.clone();
        thread::spawn(move || {
            if let Err(e) = run(rx, &status, &repaint) {
                status.lock().unwrap().error = Some(e.to_string());
            }
            status.lock().unwrap().running = false;
            repaint();
        });
    }
}

fn run(rx: Receiver<Cmd>, status: &Mutex<Status>, repaint: &dyn Fn()) -> libmpv2::Result<()> {
    let mpv = Mpv::with_initializer(|init| {
        init.set_property("vo", "gpu")?;
        init.set_property("hwdec", "auto-safe")?;
        init.set_property("force-window", "yes")?;
        init.set_property("osc", true)?;
        init.set_property("input-default-bindings", true)?;
        init.set_property("input-vo-keyboard", true)?;
        init.set_property("keep-open", "no")?;
        init.set_property("cache", "yes")?;
        init.set_property("demuxer-max-bytes", "128MiB")?;
        init.set_property("demuxer-lavf-o", "fflags=+discardcorrupt")?;
        // 放送 TS 的字幕轨道没有语言标签，sid=auto 不会选中它；fallback=yes 时没有匹配语言也会选
        init.set_property("subs-fallback", "yes")?;
        init.set_property("title", "${media-title} - EPGPlayer For Desktop")?;
        Ok(())
    })?;

    loop {
        loop {
            match rx.try_recv() {
                Ok(Cmd::Load { url, title }) => {
                    mpv.set_property("force-media-title", title.as_str())?;
                    mpv.command("loadfile", &[url.as_str(), "replace"])?;
                    let mut s = status.lock().unwrap();
                    s.title = Some(title);
                    s.error = None;
                    drop(s);
                    repaint();
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        match mpv.wait_event(0.05) {
            Some(Ok(Event::Shutdown)) => return Ok(()),
            Some(Ok(Event::EndFile(reason))) => {
                if reason == mpv_end_file_reason::Error {
                    status.lock().unwrap().error = Some(
                        crate::i18n::tr(
                            "再生に失敗しました（ストリームを開けないか、中断されました）",
                            "播放失败（流无法打开或中断）",
                            "Playback failed (stream could not be opened or was interrupted)",
                        )
                        .into(),
                    );
                    repaint();
                }
            }
            _ => {}
        }
    }
}
