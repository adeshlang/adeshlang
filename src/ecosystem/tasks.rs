use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct TaskDefinition {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub dependencies: Vec<String>,
    pub parallel: Vec<String>,
    pub condition: Option<String>,
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
    pub platform: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct TaskRunner {
    pub tasks: BTreeMap<String, TaskDefinition>,
}

impl TaskRunner {
    pub fn register(&mut self, task: TaskDefinition) {
        self.tasks.insert(task.name.clone(), task);
    }

    pub fn is_up_to_date(&self, task: &TaskDefinition) -> bool {
        if task.inputs.is_empty() || task.outputs.is_empty() {
            return false;
        }

        for output in &task.outputs {
            if !output.exists() {
                return false;
            }
        }

        let mut max_input_mtime = None;
        for input in &task.inputs {
            if let Ok(metadata) = input.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if max_input_mtime.is_none() || Some(modified) > max_input_mtime {
                        max_input_mtime = Some(modified);
                    }
                }
            } else {
                return false;
            }
        }

        let mut min_output_mtime = None;
        for output in &task.outputs {
            if let Ok(metadata) = output.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if min_output_mtime.is_none() || Some(modified) < min_output_mtime {
                        min_output_mtime = Some(modified);
                    }
                }
            }
        }

        match (max_input_mtime, min_output_mtime) {
            (Some(input_time), Some(output_time)) => output_time >= input_time,
            _ => false,
        }
    }

    pub fn run(&self, task_name: &str) -> Result<(), String> {
        let task = self
            .tasks
            .get(task_name)
            .ok_or_else(|| format!("unknown task: {task_name}"))?;

        for dep in &task.dependencies {
            self.run(dep)?;
        }

        if self.is_up_to_date(task) {
            println!("Task '{}' is up-to-date, skipping.", task_name);
            return Ok(());
        }

        if let Some(ref platform) = task.platform {
            let current_os = std::env::consts::OS;
            if platform != current_os && platform != "any" {
                println!(
                    "Task '{}' platform requirement '{}' does not match current OS '{}'. Skipping.",
                    task_name, platform, current_os
                );
                return Ok(());
            }
        }

        if !task.parallel.is_empty() {
            let mut handles = Vec::new();
            for parallel_task_name in &task.parallel {
                let parallel_task_name = parallel_task_name.clone();
                let runner = self.clone();
                let handle = std::thread::spawn(move || runner.run(&parallel_task_name));
                handles.push(handle);
            }
            for handle in handles {
                handle
                    .join()
                    .map_err(|_| "parallel execution thread panicked".to_string())??;
            }
        }

        let mut command = Command::new(&task.command);
        command.args(&task.args);
        command.envs(&task.env);
        command.stdin(Stdio::inherit());
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
        let status = command.status().map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("task {task_name} failed with status {status}"))
        }
    }
}
