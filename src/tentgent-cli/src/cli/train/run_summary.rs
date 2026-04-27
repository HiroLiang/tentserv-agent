use console::style;
use serde_json::Value;

#[derive(Default)]
pub struct RunSummary {
    train_reports: usize,
    eval_reports: usize,
    final_train_step: Option<u64>,
    final_train_loss: Option<f64>,
    final_learning_rate: Option<f64>,
    final_tokens_per_sec: Option<f64>,
    final_iterations_per_sec: Option<f64>,
    peak_memory_gb: Option<f64>,
    trained_tokens: Option<u64>,
    best_eval_step: Option<u64>,
    best_eval_loss: Option<f64>,
}

impl RunSummary {
    pub fn record_event(&mut self, event: &Value) {
        match event.get("type").and_then(Value::as_str) {
            Some("train") => self.record_train(event),
            Some("eval") => self.record_eval(event),
            _ => {}
        }
    }

    pub fn has_training_signal(&self) -> bool {
        self.train_reports > 0 || self.eval_reports > 0 || self.peak_memory_gb.is_some()
    }

    fn record_train(&mut self, event: &Value) {
        self.train_reports += 1;
        self.final_train_step = event.get("step").and_then(Value::as_u64);
        self.final_train_loss = event.get("loss").and_then(Value::as_f64);
        self.final_learning_rate = event.get("learning_rate").and_then(Value::as_f64);
        self.final_tokens_per_sec = event.get("tokens_per_sec").and_then(Value::as_f64);
        self.final_iterations_per_sec = event.get("iterations_per_sec").and_then(Value::as_f64);
        self.trained_tokens = event.get("trained_tokens").and_then(Value::as_u64);

        if let Some(memory) = event.get("peak_memory_gb").and_then(Value::as_f64) {
            self.peak_memory_gb = Some(self.peak_memory_gb.map_or(memory, |peak| peak.max(memory)));
        }
    }

    fn record_eval(&mut self, event: &Value) {
        self.eval_reports += 1;
        let Some(loss) = event.get("loss").and_then(Value::as_f64) else {
            return;
        };

        if self.best_eval_loss.is_none_or(|best| loss < best) {
            self.best_eval_loss = Some(loss);
            self.best_eval_step = event.get("step").and_then(Value::as_u64);
        }
    }
}

pub fn render_run_summary(summary: &RunSummary) {
    if !summary.has_training_signal() {
        return;
    }

    println!(
        "{} {}",
        style("==>").cyan().bold(),
        style("LoRA run summary").bold()
    );
    println!("train reports: {}", summary.train_reports);

    if let Some(loss) = summary.final_train_loss {
        let step = summary
            .final_train_step
            .map(|step| format!(" at step {step}"))
            .unwrap_or_default();
        println!("final train loss: {loss:.3}{step}");
    }
    if let Some(loss) = summary.best_eval_loss {
        let step = summary
            .best_eval_step
            .map(|step| format!(" at step {step}"))
            .unwrap_or_default();
        println!("best eval loss: {loss:.3}{step}");
    }
    if let Some(memory) = summary.peak_memory_gb {
        println!("peak training memory: {memory:.2} GB");
    }
    if let Some(tokens) = summary.final_tokens_per_sec {
        println!("final throughput: {tokens:.1} tokens/sec");
    }
    if let Some(iterations) = summary.final_iterations_per_sec {
        println!("final speed: {iterations:.3} it/sec");
    }
    if let Some(tokens) = summary.trained_tokens {
        println!("trained tokens: {tokens}");
    }
    if let Some(learning_rate) = summary.final_learning_rate {
        println!("final learning rate: {learning_rate:.3e}");
    }
}
