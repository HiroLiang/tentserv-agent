use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;

use crate::features::runtime::domain::RuntimeEntrypoint;
use crate::features::runtime::ports::RuntimeExecutableResolver;
use crate::foundation::error::{KernelError, KernelResult};

use super::super::domain::{
    ChatFinishReason, ChatRequest, ChatResponse, ChatRuntimeTarget, ChatStreamEvent,
};
use super::super::ports::{ChatPortFuture, ChatRuntimeClient, ChatRuntimeRequest};

/// Executes prepared chat requests through the `tentgent-chat-once` Python entrypoint.
pub struct PythonChatOnceRuntimeClient<'a> {
    executable_resolver: &'a dyn RuntimeExecutableResolver,
}

impl<'a> PythonChatOnceRuntimeClient<'a> {
    pub fn new(executable_resolver: &'a dyn RuntimeExecutableResolver) -> Self {
        Self {
            executable_resolver,
        }
    }

    fn generate_chat_blocking(&self, request: ChatRuntimeRequest) -> KernelResult<ChatResponse> {
        let output = self
            .command_for_request(&request, false)?
            .output()
            .map_err(|error| chat_runtime_error(format!("failed to run chat runtime: {error}")))?;

        if !output.status.success() {
            return Err(chat_runtime_error(format_process_failure(
                "chat runtime exited",
                output.status.code(),
                &output.stderr,
            )));
        }

        Ok(ChatResponse {
            text: decode_stdout_text(&output.stdout),
            finish_reason: ChatFinishReason::Stop,
        })
    }

    fn stream_chat_blocking(
        &self,
        request: ChatRuntimeRequest,
        sink: &mut dyn FnMut(ChatStreamEvent),
    ) -> KernelResult<ChatResponse> {
        let mut child = self
            .command_for_request(&request, true)?
            .spawn()
            .map_err(|error| {
                chat_runtime_error(format!("failed to start chat runtime: {error}"))
            })?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| chat_runtime_error("failed to capture chat runtime stdout"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| chat_runtime_error("failed to capture chat runtime stderr"))?;

        let stderr_task = thread::spawn(move || {
            let mut buffer = Vec::new();
            stderr.read_to_end(&mut buffer).map(|_| buffer)
        });

        let mut collected = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = stdout.read(&mut buffer).map_err(|error| {
                chat_runtime_error(format!("failed to read chat stdout: {error}"))
            })?;
            if read == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&buffer[..read]).to_string();
            if !text.is_empty() {
                sink(ChatStreamEvent::Delta { text });
            }
            collected.extend_from_slice(&buffer[..read]);
        }

        let status = child.wait().map_err(|error| {
            chat_runtime_error(format!("failed to wait for chat runtime: {error}"))
        })?;
        let stderr_bytes = stderr_task
            .join()
            .map_err(|_| chat_runtime_error("failed to join chat stderr reader"))?
            .map_err(|error| chat_runtime_error(format!("failed to read chat stderr: {error}")))?;

        if !status.success() {
            let message =
                format_process_failure("chat runtime exited", status.code(), &stderr_bytes);
            sink(ChatStreamEvent::Error {
                code: "chat_runtime_failed".to_string(),
                message: message.clone(),
            });
            return Err(chat_runtime_error(message));
        }

        let finish_reason = ChatFinishReason::Stop;
        sink(ChatStreamEvent::Done {
            finish_reason: finish_reason.clone(),
        });

        Ok(ChatResponse {
            text: decode_stdout_text(&collected),
            finish_reason,
        })
    }

    fn command_for_request(
        &self,
        request: &ChatRuntimeRequest,
        stream: bool,
    ) -> KernelResult<Command> {
        let entrypoint = self
            .executable_resolver
            .entrypoint_path(&request.runtime, RuntimeEntrypoint::ChatOnce)?;
        let model_ref = local_model_ref(&request.request)?;

        let mut command = Command::new(entrypoint);
        command
            .current_dir(&request.runtime.project_dir)
            .arg("--model-ref")
            .arg(model_ref)
            .arg("--home")
            .arg(&request.layout.home_dir)
            .env("TENTGENT_HOME", &request.layout.home_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(max_tokens) = request.request.options.max_tokens {
            command.arg("--max-tokens").arg(max_tokens.to_string());
        }
        if let Some(temperature) = request.request.options.temperature {
            command.arg("--temperature").arg(temperature.to_string());
        }
        if let Some(adapter) = &request.request.target.adapter {
            command
                .arg("--adapter-ref")
                .arg(adapter.adapter_ref.as_str());
        }
        if stream {
            command.arg("--stream");
        }
        for message in &request.request.prompt.messages {
            command
                .arg("--message")
                .arg(format!("{}:{}", message.role, message.content));
        }

        Ok(command)
    }
}

impl ChatRuntimeClient for PythonChatOnceRuntimeClient<'_> {
    fn generate_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move { self.generate_chat_blocking(request) })
    }

    fn stream_chat<'a>(
        &'a self,
        request: ChatRuntimeRequest,
        sink: &'a mut dyn FnMut(ChatStreamEvent),
    ) -> ChatPortFuture<'a, ChatResponse> {
        Box::pin(async move { self.stream_chat_blocking(request, sink) })
    }
}

fn local_model_ref(request: &ChatRequest) -> KernelResult<&str> {
    match &request.target.runtime {
        ChatRuntimeTarget::LocalModel { model_ref, .. } => Ok(model_ref.as_str()),
        ChatRuntimeTarget::CloudProvider { .. } => Err(KernelError::UnsupportedTarget(
            "tentgent-chat-once requires a local model target".to_string(),
        )),
    }
}

fn decode_stdout_text(stdout: &[u8]) -> String {
    String::from_utf8_lossy(stdout)
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

fn format_process_failure(prefix: &str, code: Option<i32>, stderr: &[u8]) -> String {
    let status = code
        .map(|code| format!("with status {code}"))
        .unwrap_or_else(|| "without an exit status".to_string());
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{prefix} {status}")
    } else {
        format!("{prefix} {status}: {stderr}")
    }
}

fn chat_runtime_error(message: impl Into<String>) -> KernelError {
    KernelError::ChatRuntimeUnavailable(message.into())
}
