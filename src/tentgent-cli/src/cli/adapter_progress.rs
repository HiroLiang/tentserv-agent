use indicatif::{ProgressBar, ProgressStyle};
use tentgent_kernel::features::adapter::domain::HfAdapterPullProgress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PullProgressMode {
    Spinner,
    Files,
    Bytes,
}

pub(super) struct PullProgress {
    bar: ProgressBar,
    repo_id: String,
    mode: PullProgressMode,
}

impl PullProgress {
    pub(super) fn new(repo_id: &str, revision: Option<&str>) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner} {msg} [{elapsed_precise}]")
                .expect("valid adapter pull spinner template"),
        );
        bar.set_message(match revision {
            Some(revision) => {
                format!("Resolving adapter {repo_id} @ {revision} from Hugging Face")
            }
            None => format!("Resolving adapter {repo_id} from Hugging Face"),
        });
        bar.enable_steady_tick(std::time::Duration::from_millis(100));

        Self {
            bar,
            repo_id: repo_id.to_string(),
            mode: PullProgressMode::Spinner,
        }
    }

    pub(super) fn update(&mut self, event: HfAdapterPullProgress) {
        if event.finished {
            return;
        }

        if event.unit == "B" {
            self.switch_mode(PullProgressMode::Bytes);
            if let Some(total) = event.total {
                self.bar.set_length(total);
            }
            self.bar.set_position(event.position);
            self.bar.set_message(match event.description.as_str() {
                "" | "Downloading (incomplete total...)" => {
                    format!("Downloading adapter {}", self.repo_id)
                }
                description => description.to_string(),
            });
            return;
        }

        self.switch_mode(PullProgressMode::Files);
        if let Some(total) = event.total {
            self.bar.set_length(total);
        }
        self.bar.set_position(event.position);
        self.bar.set_message(if event.description.is_empty() {
            format!("Fetching adapter files for {}", self.repo_id)
        } else {
            event.description
        });
    }

    pub(super) fn finish(&self) {
        self.bar.finish_and_clear();
    }

    fn switch_mode(&mut self, mode: PullProgressMode) {
        if self.mode == mode {
            return;
        }

        self.mode = mode;
        match mode {
            PullProgressMode::Spinner => {}
            PullProgressMode::Files => {
                self.bar.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.cyan} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len}",
                    )
                    .expect("valid adapter file progress template")
                    .progress_chars("=> "),
                );
            }
            PullProgressMode::Bytes => {
                self.bar.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.cyan} {msg} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} {bytes_per_sec} ETA {eta_precise}",
                    )
                    .expect("valid adapter byte progress template")
                    .progress_chars("=> "),
                );
            }
        }
    }
}
