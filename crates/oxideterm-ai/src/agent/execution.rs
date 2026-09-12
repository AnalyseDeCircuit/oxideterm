use std::future::Future;

use super::{AgentError, AgentPermit, AgentRunRef, AgentRuntime, AgentState};

/// One loop owns one context. Waiting releases only the runnable slot, never a remote lease.
pub struct AgentExecution {
    pub runtime: AgentRuntime,
    pub run: AgentRunRef,
    child: bool,
    permit: Option<AgentPermit>,
    pub event_cursor: u64,
    pub deferred_user_messages: Vec<super::AgentText>,
    pub resources: super::AgentResourceCoordinator,
}

impl AgentExecution {
    pub fn new(
        runtime: AgentRuntime,
        run: AgentRunRef,
        resources: super::AgentResourceCoordinator,
    ) -> Result<Self, AgentError> {
        let child = runtime.snapshot(&run)?.parent_id.is_some();
        Ok(Self {
            runtime,
            run,
            child,
            permit: None,
            event_cursor: 0,
            deferred_user_messages: Vec::new(),
            resources,
        })
    }

    pub fn is_child(&self) -> bool {
        self.child
    }

    pub async fn ready(&mut self) -> Result<(), AgentError> {
        if *self.runtime.cancellation(&self.run)?.borrow() {
            return Err(AgentError::Cancelled);
        }
        if self.child && self.permit.is_none() {
            self.runtime.set_state(&self.run, AgentState::Queued)?;
            self.permit = Some(self.runtime.acquire(&self.run).await?);
        } else {
            self.runtime.set_state(&self.run, AgentState::Running)?;
        }
        Ok(())
    }

    pub async fn wait<F: Future>(
        &mut self,
        state: AgentState,
        future: F,
    ) -> Result<F::Output, AgentError> {
        self.runtime.set_state(&self.run, state)?;
        self.permit.take();
        let mut cancelled = self.runtime.cancellation(&self.run)?;
        if *cancelled.borrow() {
            return Err(AgentError::Cancelled);
        }
        let output = tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(AgentError::Cancelled),
            output = future => output,
        };
        self.ready().await?;
        Ok(output)
    }

    pub async fn cancellable<F: Future>(&self, future: F) -> Result<F::Output, AgentError> {
        let mut cancelled = self.runtime.cancellation(&self.run)?;
        if *cancelled.borrow() {
            return Err(AgentError::Cancelled);
        }
        tokio::select! {
            biased;
            _ = cancelled.changed() => Err(AgentError::Cancelled),
            output = future => Ok(output),
        }
    }
}

impl Drop for AgentExecution {
    fn drop(&mut self) {
        if let Ok(snapshot) = self.runtime.snapshot(&self.run) {
            if !snapshot.state.is_terminal() {
                if !self.child {
                    let _ = self.runtime.cancel_group(&self.run);
                }
                let _ = self.runtime.complete(
                    &self.run,
                    super::AgentResult {
                        summary: super::AgentText::default(),
                        evidence: Vec::new(),
                        actions: Vec::new(),
                        unfinished: Vec::new(),
                        error_code: Some("agent_interrupted".into()),
                    },
                );
            }
        }
    }
}
