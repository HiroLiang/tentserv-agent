use super::config::{LoraTrainBackend, LoraTrainPlan, TrainPlanStatus};

#[derive(Debug, Clone, Default)]
pub struct LoraTrainOverrides {
    pub max_seq_length: Option<u32>,
    pub mask_prompt: Option<bool>,
    pub rank: Option<u32>,
    pub learning_rate: Option<f64>,
    pub batch_size: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
    pub max_steps: Option<u32>,
    pub seed: Option<u64>,
    pub mlx_num_layers: Option<u32>,
    pub mlx_grad_checkpoint: Option<bool>,
    pub peft_load_in_4bit: Option<bool>,
    pub peft_load_in_8bit: Option<bool>,
}

impl LoraTrainOverrides {
    pub fn apply_to(self, plan: &mut LoraTrainPlan) {
        if let Some(value) = self.max_seq_length {
            plan.dataset.max_seq_length = value;
        }
        if let Some(value) = self.mask_prompt {
            plan.dataset.mask_prompt = value;
        }
        if let Some(value) = self.rank {
            plan.lora.rank = value;
            if plan.backend == Some(LoraTrainBackend::Peft) {
                plan.lora.alpha = Some(value * 2);
            }
        }
        if let Some(value) = self.learning_rate {
            plan.optimization.learning_rate = value;
        }
        if let Some(value) = self.batch_size {
            plan.optimization.batch_size = value;
        }
        if let Some(value) = self.gradient_accumulation_steps {
            plan.optimization.gradient_accumulation_steps = value;
        }
        if let Some(value) = self.max_steps {
            plan.optimization.max_steps = value;
        }
        if let Some(value) = self.seed {
            plan.optimization.seed = value;
        }
        let has_mlx_overrides = self.mlx_num_layers.is_some() || self.mlx_grad_checkpoint.is_some();
        let has_peft_overrides =
            self.peft_load_in_4bit.is_some() || self.peft_load_in_8bit.is_some();

        if let Some(mlx) = plan.backend_config.mlx.as_mut() {
            if let Some(value) = self.mlx_num_layers {
                mlx.num_layers = value;
            }
            if let Some(value) = self.mlx_grad_checkpoint {
                mlx.grad_checkpoint = value;
            }
        } else if has_mlx_overrides {
            plan.warnings
                .push("MLX override ignored because selected backend is not mlx".to_string());
        }
        if let Some(peft) = plan.backend_config.peft.as_mut() {
            if let Some(value) = self.peft_load_in_4bit {
                peft.load_in_4bit = value;
            }
            if let Some(value) = self.peft_load_in_8bit {
                peft.load_in_8bit = value;
            }
            if peft.load_in_4bit && peft.load_in_8bit {
                plan.status = TrainPlanStatus::Blocked;
                plan.blockers
                    .push("PEFT cannot load in both 4-bit and 8-bit modes".to_string());
            }
        } else if has_peft_overrides {
            plan.warnings
                .push("PEFT override ignored because selected backend is not peft".to_string());
        }
    }
}
