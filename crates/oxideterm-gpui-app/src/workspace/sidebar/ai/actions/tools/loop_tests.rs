mod local_command_tests {
    use super::*;

    #[tokio::test]
    async fn local_command_cleanup_and_request_ownership_are_independent() {
        use oxideterm_ai::agent::{AgentModel, AgentResourceCoordinator, AgentRuntime, AgentScope, AgentToolLease};
        let runtime = AgentRuntime::new(1);
        let run = runtime.create_group("local-command".into(), AgentModel {
            provider_id: "provider".into(), model: "model".into(),
        }, AgentScope::default(), 8);
        let resources = AgentResourceCoordinator::default();
        let key = resources.workspace_resource();
        for early_response in [None, Some(true), Some(false)] {
            let lease = resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
            let lease = AgentToolLease::new(resources.clone(), lease);
            lease.dispatched();
            let (release, wait) = tokio::sync::oneshot::channel();
            let process = tokio::spawn(async move {
                wait.await.unwrap();
                run_local_ai_command("exit 7", None, false, None).await
            });
            let mut task = AiOwnedCommandTask::new(process, vec![lease.clone()]);
            if let Some(success) = early_response { lease.finish_response(success); }
            assert_eq!(resources.owned_by(&key, &run).is_some(), early_response.is_none(), "ownership ends with the request, independently of process outcome");
            release.send(()).unwrap();
            let action = (&mut task.process).await.unwrap();
            assert!(!action.ok);
            assert_eq!(action.data["exitCode"], 7);
            task.finish(&action);
            lease.finish_response(action.ok);
            drop(task);
            let next = tokio::time::timeout(Duration::from_secs(1), resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap())).await.unwrap().unwrap();
            resources.complete(&next);
        }

        let lease = resources.acquire(key.clone(), run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
        let lease = AgentToolLease::new(resources.clone(), lease);
        lease.dispatched();
        let task = AiOwnedCommandTask::new(tokio::spawn(std::future::pending()), vec![lease]);
        drop(task);
        let next = resources.acquire(key, run.clone(), runtime.cancellation(&run).unwrap()).await.unwrap();
        assert!(resources.owns(&next));
        assert!(resources.complete(&next));
    }
}
