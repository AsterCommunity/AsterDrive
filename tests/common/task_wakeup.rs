//! Observe the actual Forge runner's subscription without exposing its channel internals.

use std::time::Duration;

use aster_forge_tasks::{
    TaskConsumer, TaskConsumerConfig, TaskDispatchOutcome, TaskRuntimeStatus, TaskWakeup,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct ProbeConsumer {
    wakeup: TaskWakeup,
    claims: mpsc::UnboundedSender<()>,
}

#[async_trait::async_trait]
impl TaskConsumer for ProbeConsumer {
    type Task = ();
    type Error = &'static str;

    fn name(&self) -> &'static str {
        "test_wakeup_probe"
    }

    fn concurrency(&self) -> usize {
        1
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_hours(1)
    }

    fn wakeup(&self) -> &TaskWakeup {
        &self.wakeup
    }

    async fn claim(&self, _: usize) -> Result<Vec<()>, Self::Error> {
        self.claims.send(()).map_err(|_| "probe receiver closed")?;
        Ok(Vec::new())
    }

    async fn execute(
        &self,
        (): (),
        _: CancellationToken,
    ) -> Result<TaskDispatchOutcome, Self::Error> {
        unreachable!("probe never claims tasks")
    }

    async fn release_unstarted(&self, (): ()) -> Result<(), Self::Error> {
        unreachable!("probe never claims tasks")
    }
}

pub struct TaskWakeupProbe {
    stop: CancellationToken,
    runner:
        tokio::task::JoinHandle<aster_forge_tasks::Result<aster_forge_tasks::TaskConsumerReport>>,
    claims: mpsc::UnboundedReceiver<()>,
}

impl TaskWakeupProbe {
    pub async fn new(wakeup: TaskWakeup) -> Self {
        let (sender, claims) = mpsc::unbounded_channel();
        let stop = CancellationToken::new();
        let runner = tokio::spawn(aster_forge_tasks::run_task_consumer(
            ProbeConsumer {
                wakeup,
                claims: sender,
            },
            stop.clone(),
            TaskConsumerConfig::default(),
            TaskRuntimeStatus::default(),
        ));
        let mut probe = Self {
            stop,
            runner,
            claims,
        };
        // Consume startup claim only after subscription is established.
        probe.notified().await;
        probe
    }

    pub async fn notified(&mut self) {
        tokio::time::timeout(Duration::from_secs(2), self.claims.recv())
            .await
            .expect("consumer should be notified without its hourly timer")
            .expect("probe runner should remain alive");
    }
}

impl Drop for TaskWakeupProbe {
    fn drop(&mut self) {
        self.stop.cancel();
        // This synthetic runner performs no persistence or product I/O.
        self.runner.abort();
    }
}
