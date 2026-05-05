use super::{
    navigator::{NavigatorListKind, NavigatorRow},
    runtime_action::RuntimeActionKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeWizardFlow {
    CreateServer,
    CreateLoraPlan,
}

impl RuntimeWizardFlow {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::CreateServer => "Create server",
            Self::CreateLoraPlan => "Create LoRA plan",
        }
    }

    pub(super) fn create_action(self) -> RuntimeActionKind {
        match self {
            Self::CreateServer => RuntimeActionKind::ServerCreate,
            Self::CreateLoraPlan => RuntimeActionKind::TrainPlanCreate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeWizardStep {
    PickModel,
    PickDataset,
    PickBackend,
    ServerConfig,
    PlanBasics,
    AdvancedChoice,
    AdvancedFields,
    Review,
}

impl RuntimeWizardStep {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::PickModel => "Choose model",
            Self::PickDataset => "Choose dataset",
            Self::PickBackend => "Choose backend",
            Self::ServerConfig => "Configure server",
            Self::PlanBasics => "Plan details",
            Self::AdvancedChoice => "Advanced settings",
            Self::AdvancedFields => "Edit advanced",
            Self::Review => "Review",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeWizardBackend {
    Auto,
    Mlx,
    Peft,
    Manual,
}

impl RuntimeWizardBackend {
    pub(super) const ALL: [Self; 4] = [Self::Auto, Self::Mlx, Self::Peft, Self::Manual];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Mlx => "mlx",
            Self::Peft => "peft",
            Self::Manual => "advanced/manual",
        }
    }

    fn value(self, manual: &str) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Mlx => "mlx".to_string(),
            Self::Peft => "peft".to_string(),
            Self::Manual => manual.trim().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeWizardAdvancedChoice {
    Defaults,
    Customize,
}

impl RuntimeWizardAdvancedChoice {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Defaults => "use defaults",
            Self::Customize => "customize advanced",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimePickerMode {
    Local,
    Manual,
}

impl RuntimePickerMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Local => "local picker",
            Self::Manual => "advanced manual ref",
        }
    }

    pub(super) fn toggle(&mut self) {
        *self = match self {
            Self::Local => Self::Manual,
            Self::Manual => Self::Local,
        };
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimePickerState {
    pub(super) kind: NavigatorListKind,
    pub(super) mode: RuntimePickerMode,
    pub(super) selected_index: usize,
    pub(super) selected_ref: Option<String>,
    pub(super) filter: String,
    pub(super) manual_value: String,
    pub(super) manual_cursor: usize,
}

impl RuntimePickerState {
    fn new(kind: NavigatorListKind, selected_ref: Option<String>) -> Self {
        Self {
            kind,
            mode: RuntimePickerMode::Local,
            selected_index: 0,
            selected_ref,
            filter: String::new(),
            manual_value: String::new(),
            manual_cursor: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RuntimePreviewStatus {
    NotRun,
    Running,
    Ready,
    Stale,
    Blocked,
    Error,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimePreviewState {
    pub(super) status: RuntimePreviewStatus,
    pub(super) lines: Vec<(String, String)>,
    pub(super) message: Option<String>,
}

impl Default for RuntimePreviewState {
    fn default() -> Self {
        Self {
            status: RuntimePreviewStatus::NotRun,
            lines: Vec::new(),
            message: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeWizardDraft {
    pub(super) runtime_ref: String,
    pub(super) model_ref: String,
    pub(super) dataset_ref: String,
    pub(super) name: String,
    pub(super) backend: RuntimeWizardBackend,
    pub(super) manual_backend: String,
    pub(super) host: String,
    pub(super) port: String,
    pub(super) lazy_load: bool,
    pub(super) idle_seconds: String,
    pub(super) advanced_choice: RuntimeWizardAdvancedChoice,
    pub(super) max_seq_length: String,
    pub(super) rank: String,
    pub(super) learning_rate: String,
    pub(super) batch_size: String,
    pub(super) gradient_accumulation_steps: String,
    pub(super) max_steps: String,
    pub(super) seed: String,
    pub(super) mask_prompt: Option<bool>,
    pub(super) mlx_num_layers: String,
    pub(super) mlx_grad_checkpoint: Option<bool>,
    pub(super) peft_load_in_4bit: Option<bool>,
    pub(super) peft_load_in_8bit: Option<bool>,
}

impl Default for RuntimeWizardDraft {
    fn default() -> Self {
        Self {
            runtime_ref: String::new(),
            model_ref: String::new(),
            dataset_ref: String::new(),
            name: String::new(),
            backend: RuntimeWizardBackend::Auto,
            manual_backend: String::new(),
            host: "127.0.0.1".to_string(),
            port: "18780".to_string(),
            lazy_load: true,
            idle_seconds: String::new(),
            advanced_choice: RuntimeWizardAdvancedChoice::Defaults,
            max_seq_length: String::new(),
            rank: String::new(),
            learning_rate: String::new(),
            batch_size: String::new(),
            gradient_accumulation_steps: String::new(),
            max_steps: String::new(),
            seed: String::new(),
            mask_prompt: None,
            mlx_num_layers: String::new(),
            mlx_grad_checkpoint: None,
            peft_load_in_4bit: None,
            peft_load_in_8bit: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeWizardState {
    pub(super) flow: RuntimeWizardFlow,
    pub(super) step: RuntimeWizardStep,
    pub(super) draft: RuntimeWizardDraft,
    pub(super) picker: Option<RuntimePickerState>,
    pub(super) preview: RuntimePreviewState,
    pub(super) validation_errors: Vec<String>,
    pub(super) dirty_since_preview: bool,
    pub(super) selected_field: usize,
    pub(super) selected_review_row: usize,
}

impl RuntimeWizardState {
    pub(super) fn new(action: RuntimeActionKind, selected: Option<&NavigatorRow>) -> Option<Self> {
        let mut draft = RuntimeWizardDraft::default();
        let (flow, step, picker) = match action {
            RuntimeActionKind::ServerCreate => (
                RuntimeWizardFlow::CreateServer,
                RuntimeWizardStep::PickModel,
                Some(RuntimePickerState::new(NavigatorListKind::Models, None)),
            ),
            RuntimeActionKind::ServerCreateFromModel => {
                if let Some(row) = selected {
                    draft.runtime_ref = row.item_ref.clone();
                }
                (
                    RuntimeWizardFlow::CreateServer,
                    RuntimeWizardStep::ServerConfig,
                    None,
                )
            }
            RuntimeActionKind::TrainPlanCreate | RuntimeActionKind::TrainPlanPreview => (
                RuntimeWizardFlow::CreateLoraPlan,
                RuntimeWizardStep::PickModel,
                Some(RuntimePickerState::new(NavigatorListKind::Models, None)),
            ),
            RuntimeActionKind::TrainPlanCreateFromDataset => {
                if let Some(row) = selected {
                    draft.dataset_ref = row.item_ref.clone();
                }
                (
                    RuntimeWizardFlow::CreateLoraPlan,
                    RuntimeWizardStep::PickModel,
                    Some(RuntimePickerState::new(NavigatorListKind::Models, None)),
                )
            }
            _ => return None,
        };
        Some(Self {
            flow,
            step,
            draft,
            picker,
            preview: RuntimePreviewState::default(),
            validation_errors: Vec::new(),
            dirty_since_preview: true,
            selected_field: 0,
            selected_review_row: 0,
        })
    }

    pub(super) fn set_step(&mut self, step: RuntimeWizardStep) {
        self.step = step;
        self.selected_field = 0;
        self.selected_review_row = 0;
        self.picker = match step {
            RuntimeWizardStep::PickModel => {
                let selected = match self.flow {
                    RuntimeWizardFlow::CreateServer => self.draft.runtime_ref.clone(),
                    RuntimeWizardFlow::CreateLoraPlan => self.draft.model_ref.clone(),
                };
                Some(RuntimePickerState::new(
                    NavigatorListKind::Models,
                    empty_to_none(selected),
                ))
            }
            RuntimeWizardStep::PickDataset => Some(RuntimePickerState::new(
                NavigatorListKind::Datasets,
                empty_to_none(self.draft.dataset_ref.clone()),
            )),
            _ => None,
        };
    }

    pub(super) fn mark_dirty(&mut self) {
        if matches!(self.flow, RuntimeWizardFlow::CreateLoraPlan) {
            self.dirty_since_preview = true;
            if matches!(self.preview.status, RuntimePreviewStatus::Ready) {
                self.preview.status = RuntimePreviewStatus::Stale;
                self.preview.message = Some("fields changed after preview".to_string());
            }
        }
    }

    pub(super) fn review_rows(&self) -> Vec<RuntimeWizardReviewRow> {
        match self.flow {
            RuntimeWizardFlow::CreateServer => vec![
                RuntimeWizardReviewRow::Field("runtime_ref", self.draft.runtime_ref.clone()),
                RuntimeWizardReviewRow::Field("host", self.draft.host.clone()),
                RuntimeWizardReviewRow::Field("port", self.draft.port.clone()),
                RuntimeWizardReviewRow::Field("lazy_load", self.draft.lazy_load.to_string()),
                RuntimeWizardReviewRow::Field(
                    "idle_seconds",
                    empty_label(&self.draft.idle_seconds),
                ),
                RuntimeWizardReviewRow::Submit,
            ],
            RuntimeWizardFlow::CreateLoraPlan => {
                let mut rows = vec![
                    RuntimeWizardReviewRow::Field("model_ref", self.draft.model_ref.clone()),
                    RuntimeWizardReviewRow::Field("dataset_ref", self.draft.dataset_ref.clone()),
                    RuntimeWizardReviewRow::Field("backend", self.backend_value()),
                    RuntimeWizardReviewRow::Field("name", empty_label(&self.draft.name)),
                    RuntimeWizardReviewRow::Field(
                        "advanced",
                        self.draft.advanced_choice.label().to_string(),
                    ),
                    RuntimeWizardReviewRow::Preview,
                    RuntimeWizardReviewRow::Submit,
                ];
                if self.draft.advanced_choice == RuntimeWizardAdvancedChoice::Customize {
                    rows.insert(
                        5,
                        RuntimeWizardReviewRow::Field(
                            "advanced_fields",
                            "custom overrides".to_string(),
                        ),
                    );
                }
                rows
            }
        }
    }

    pub(super) fn backend_value(&self) -> String {
        self.draft.backend.value(&self.draft.manual_backend)
    }

    pub(super) fn create_values(&self) -> Vec<(&'static str, String)> {
        match self.flow {
            RuntimeWizardFlow::CreateServer => vec![
                ("runtime_ref", self.draft.runtime_ref.clone()),
                ("host", self.draft.host.clone()),
                ("port", self.draft.port.clone()),
                ("lazy_load", self.draft.lazy_load.to_string()),
                ("idle_seconds", self.draft.idle_seconds.clone()),
            ],
            RuntimeWizardFlow::CreateLoraPlan => self.lora_values(),
        }
    }

    pub(super) fn preview_values(&self) -> Vec<(&'static str, String)> {
        self.lora_values()
    }

    fn lora_values(&self) -> Vec<(&'static str, String)> {
        vec![
            ("model_ref", self.draft.model_ref.clone()),
            ("dataset_ref", self.draft.dataset_ref.clone()),
            ("name", self.draft.name.clone()),
            ("backend", self.backend_value()),
            ("max_seq_length", self.draft.max_seq_length.clone()),
            ("rank", self.draft.rank.clone()),
            ("learning_rate", self.draft.learning_rate.clone()),
            ("batch_size", self.draft.batch_size.clone()),
            (
                "gradient_accumulation_steps",
                self.draft.gradient_accumulation_steps.clone(),
            ),
            ("max_steps", self.draft.max_steps.clone()),
            ("seed", self.draft.seed.clone()),
            ("mask_prompt", optional_bool_value(self.draft.mask_prompt)),
            ("mlx_num_layers", self.draft.mlx_num_layers.clone()),
            (
                "mlx_grad_checkpoint",
                optional_bool_value(self.draft.mlx_grad_checkpoint),
            ),
            (
                "peft_load_in_4bit",
                optional_bool_value(self.draft.peft_load_in_4bit),
            ),
            (
                "peft_load_in_8bit",
                optional_bool_value(self.draft.peft_load_in_8bit),
            ),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RuntimeWizardReviewRow {
    Field(&'static str, String),
    Preview,
    Submit,
}

pub(super) fn is_runtime_wizard_action(action: RuntimeActionKind) -> bool {
    matches!(
        action,
        RuntimeActionKind::ServerCreate
            | RuntimeActionKind::ServerCreateFromModel
            | RuntimeActionKind::TrainPlanPreview
            | RuntimeActionKind::TrainPlanCreate
            | RuntimeActionKind::TrainPlanCreateFromDataset
    )
}

fn optional_bool_value(value: Option<bool>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn empty_label(value: &str) -> String {
    if value.trim().is_empty() {
        "(empty)".to_string()
    } else {
        value.to_string()
    }
}

fn empty_to_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
