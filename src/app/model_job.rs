// app/model_job.rs — what the model labels read, and the one worker that
// reads them.

use super::AppCommand;
use crate::{
    config::schema::{Config, ModelSource, Reads},
    models::{ModelReader, Models},
    providers::ProviderId,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc;

/// What the model readers are asked for.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ModelRequest {
    pub reads: Reads,
    pub source: ModelSource,
    pub ids: Vec<ProviderId>,
}

/// What to read for the model labels, or None when nothing is to be read:
/// no label part is on, or no model read is allowed.
///
/// A provider is read for when it runs (`Config::provider_active`), however
/// it signs in. One set up only with the app's own sign-in (a pasted key,
/// the menu's Cursor sign-in) reads nothing of the tool's sign-in, but its
/// settings file is read for the model label when `[reads] tool_settings`
/// allows it - that consent names the file - and its newest session when
/// `last_session` does.
pub(super) fn model_request(config: &Config) -> Option<ModelRequest> {
    let model = &config.ui.model;
    let reads = &config.reads;
    let parts_on = !model.widget.is_empty() || !model.tooltip.is_empty();
    let sessions = model.source == ModelSource::LastUsed && reads.last_session;
    if !parts_on || !(reads.tool_settings || sessions) {
        return None;
    }
    let ids = config
        .providers
        .iter()
        .filter(|p| config.provider_active(p))
        .filter_map(|p| ProviderId::from_config_id(&p.id))
        .collect();
    Some(ModelRequest {
        reads: reads.clone(),
        source: model.source,
        ids,
    })
}

/// At most one model read at a time. A file on a network drive that does not
/// answer, or a cloud placeholder waiting to download, can hold a read for a
/// long time; before this every polling cycle queued another blocking thread
/// behind it, up to the pool's 512. Now a request that comes while a read
/// runs only marks that one more read is wanted, and the running worker
/// takes it - with the request as it is by then, not as it was queued.
#[derive(Debug, Default)]
pub(super) struct ReadGate {
    busy: AtomicBool,
    again: AtomicBool,
}

impl ReadGate {
    /// A read is wanted. True when the caller is to start the worker; false
    /// when one runs and will read once more.
    pub(super) fn want(&self) -> bool {
        self.again.store(true, Ordering::SeqCst);
        !self.busy.swap(true, Ordering::SeqCst)
    }

    /// The worker is about to read: every want so far is answered by it.
    pub(super) fn begin(&self) {
        self.again.store(false, Ordering::SeqCst);
    }

    /// The worker finished a read. True when it is to read again (a want
    /// came meanwhile and no other worker took it).
    pub(super) fn finish(&self) -> bool {
        self.busy.store(false, Ordering::SeqCst);
        self.again.load(Ordering::SeqCst) && !self.busy.swap(true, Ordering::SeqCst)
    }
}

/// The model readers and what they are asked for, shared with the polling
/// cycle. The request is only ever held for a moment (the event loop takes
/// it), the reader for a whole read (the worker only), so a slow disk can
/// never stall the window.
pub(super) struct ModelJob {
    reader: Mutex<ModelReader>,
    /// What to read, and its generation: a read is shown only if no newer
    /// request came while it ran.
    request: Mutex<(u64, Option<ModelRequest>)>,
    gate: ReadGate,
}

pub(super) type SharedModelJob = Arc<ModelJob>;

impl ModelJob {
    pub(super) fn new(config: &Config) -> SharedModelJob {
        Arc::new(ModelJob {
            reader: Mutex::new(ModelReader::new()),
            request: Mutex::new((0, model_request(config))),
            gate: ReadGate::default(),
        })
    }

    pub(super) fn request(&self) -> MutexGuard<'_, (u64, Option<ModelRequest>)> {
        self.request.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The generation of the current request.
    pub(super) fn generation(&self) -> u64 {
        self.request().0
    }
}

/// Read the models on a blocking thread and hand them to the event loop as
/// `AppCommand::Models`. Nothing at all happens while nothing is requested,
/// and nothing piles up while a read is still running (`ReadGate`).
pub(super) fn spawn_model_read(
    job: &SharedModelJob,
    tx: &mpsc::Sender<AppCommand>,
    rt: &tokio::runtime::Handle,
) {
    if job.request().1.is_none() || !job.gate.want() {
        return;
    }
    let job = job.clone();
    let tx = tx.clone();
    rt.spawn_blocking(move || loop {
        job.gate.begin();
        // The request as it is now: one withdrawn while this worker waited
        // reads nothing, one changed reads what it asks for now.
        let (generation, request) = job.request().clone();
        if let Some(req) = request {
            let models = job
                .reader
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .read(&req.reads, req.source, &req.ids);
            // Changed while it read: the next round reads anew.
            if job.generation() == generation {
                let _ = tx.blocking_send(AppCommand::Models(generation, models));
            }
        }
        if !job.gate.finish() {
            break;
        }
    });
}

/// Point the model readers at what the config now asks for and read at
/// once, or, when nothing is to be read any more, drop what is shown.
/// Returns whether `models` changed here (a read lands later, as
/// `AppCommand::Models`).
pub(super) fn refresh_models(
    job: &SharedModelJob,
    config: &Config,
    models: &mut Models,
    tx: &mpsc::Sender<AppCommand>,
    rt: &tokio::runtime::Handle,
) -> bool {
    let request = model_request(config);
    let none = request.is_none();
    {
        let mut slot = job.request();
        slot.0 += 1;
        slot.1 = request;
    }
    if none {
        let changed = !models.is_empty();
        models.clear();
        changed
    } else {
        spawn_model_read(job, tx, rt);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::ModelPart;

    /// The model readers are asked for nothing unless a label part is on AND
    /// a model read is allowed; then only for the providers that run.
    #[test]
    fn model_request_needs_a_part_and_a_read() {
        let mut config = Config::default();
        config.allow_sign_in("claude");
        config.allow_sign_in("codex");
        assert!(model_request(&config).is_none());
        config.ui.model.tooltip = vec![ModelPart::Model];
        // A part alone reads nothing.
        assert!(model_request(&config).is_none());
        // Last used without its own read reads nothing either.
        config.ui.model.source = ModelSource::LastUsed;
        assert!(model_request(&config).is_none());
        config.reads.last_session = true;
        let req = model_request(&config).unwrap();
        assert_eq!(req.ids, [ProviderId::Claude, ProviderId::Codex]);
        // Settings allowed, parts off: nothing.
        config.ui.model.tooltip.clear();
        config.reads.tool_settings = true;
        assert!(model_request(&config).is_none());
        config.ui.model.widget = vec![ModelPart::Effort];
        // A provider switched off is not read.
        config
            .providers
            .iter_mut()
            .find(|p| p.id == "codex")
            .unwrap()
            .enabled = false;
        assert_eq!(model_request(&config).unwrap().ids, [ProviderId::Claude]);
    }

    /// The owner's choice (2026-11-16): a provider set up only with the
    /// app's own sign-in has its settings file read for the label under the
    /// settings consent, which names the file - and never without it.
    #[test]
    fn an_app_sign_in_reads_settings_only_under_the_settings_consent() {
        let mut config = Config::default();
        config.allow_app_sign_in("cursor");
        config.ui.model.widget = vec![ModelPart::Model];
        assert!(
            model_request(&config).is_none(),
            "nothing read without the settings consent"
        );
        config.reads.tool_settings = true;
        let req = model_request(&config).unwrap();
        assert_eq!(req.ids, [ProviderId::Cursor]);
        assert!(req.reads.tool_settings && !req.reads.last_session);
        // Taken back: nothing of it is read any more.
        config.revoke("cursor");
        assert!(model_request(&config).unwrap().ids.is_empty());
    }

    #[test]
    fn one_worker_at_a_time_and_no_want_is_lost() {
        let gate = ReadGate::default();
        // The first want starts the worker; wants while it reads do not.
        assert!(gate.want());
        gate.begin();
        assert!(!gate.want());
        assert!(!gate.want());
        // The worker reads once more for them, then stops.
        assert!(gate.finish());
        gate.begin();
        assert!(!gate.finish());
        // Idle again: the next want starts a worker.
        assert!(gate.want());
    }

    #[test]
    fn a_want_after_the_worker_let_go_starts_a_new_one_and_not_two() {
        let gate = ReadGate::default();
        assert!(gate.want());
        gate.begin();
        // The worker clears `busy` first; a want lands right then.
        gate.busy.store(false, Ordering::SeqCst);
        assert!(gate.want(), "the new want starts its own worker");
        // The old worker sees the want but the new one has it: it stops.
        assert!(gate.again.load(Ordering::SeqCst) && gate.busy.swap(true, Ordering::SeqCst));
    }
}
