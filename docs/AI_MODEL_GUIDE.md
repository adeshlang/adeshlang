# AdeshLang AI & Specialized LLM Guide

This guide covers the architecture, dataset collection, compiler validation, training, evaluation, and integration of the specialized AdeshLang language model in `ai/`.

---

## Overview & Objective

The `ai/` project builds a small, lightweight, highly accurate language model specialized exclusively for **AdeshLang v0.3.0**.

Key capabilities:
- **Understanding AdeshLang syntax & semantics**
- **Generating correct AdeshLang code**
- **Completing AdeshLang code snippets**
- **Explaining AdeshLang programs**
- **Converting requirements/pseudocode to AdeshLang**
- **Debugging and auto-fixing compiler errors**
- **Answering stdlib & CLI questions**
- **Assisting with compiler and runtime architecture**

The model is purposely small (runs locally on consumer hardware CPU/GPU) and focuses on maximum AdeshLang-specific accuracy rather than general-world knowledge.

---

## Architecture & Compiler Validation Directive

To eliminate hallucination and guarantee code correctness, the pipeline enforces **Compiler-Verified AI**:

$$\text{Knowledge Base} \longrightarrow \text{Dataset} \longrightarrow \text{Model Output} \longrightarrow \mathbf{\text{AdeshLang Compiler}} \longrightarrow \text{Verified Output}$$

All synthetic training samples and code outputs are checked directly against `adesh check` and `adesh run`.

---

## Directory Structure

```text
ai/
├── README.md
├── LICENSE
├── pyproject.toml
├── configs/          # YAML configurations (base, pretraining, finetuning, evaluation)
├── data/             # Raw, curated, processed, synthetic, and split datasets
├── knowledge/        # Extracted repository knowledge (syntax, stdlib, compiler, cli, backends)
├── datasets/         # Task datasets (instruction, code, qa, completion, debugging, conversations)
├── scripts/          # DX CLI entry points
├── src/              # Core modules (dataset, training, evaluation, inference, utils)
├── tests/            # Unit test suite
├── models/           # Exported checkpoints, ONNX, and GGUF files
└── notebooks/        # Jupyter notebooks
```

---

## Developer Workflows & Commands

### 1. Collect Repository Knowledge
Automatically extracts keywords, stdlib APIs, compiler specs, CLI flags, and code examples:
```bash
python3 -m ai.dataset.collect
# or
python3 ai/scripts/collect_knowledge.py
```

### 2. Curate & Split Datasets
Cleans, normalizes, categorizes, and produces `train.jsonl`, `val.jsonl`, `test.jsonl` in `ai/data/splits/`:
```bash
python3 -m ai.dataset.curate
```

### 3. Compiler Validation & Deduplication
Validates code snippets using the `adesh` binary and removes duplicates:
```bash
python3 -m ai.dataset.validate
python3 -m ai.dataset.deduplicate
```

### 4. Controlled Synthetic Generation
Generates synthetic exercises, debugging tasks, and pseudocode translations, validating each snippet:
```bash
python3 -m ai.dataset.synthetic
```

### 5. Training & Fine-Tuning (LoRA / QLoRA)
Train the lightweight decoder-only Transformer or apply LoRA fine-tuning:
```bash
python3 -m ai.training.train
python3 -m ai.training.finetune
```

### 6. AdeshBench Benchmark Evaluation
Evaluates models across easy, medium, hard, compiler, stdlib, debugging, and real-world tasks:
```bash
python3 -m ai.evaluation.evaluate
```

### 7. Interactive CLI & `adesh ai` Command
Generate, explain, debug, or chat from terminal:
```bash
# Python CLI
python3 -m ai.inference generate "Create a counter class"
python3 -m ai.inference explain main.adesh
python3 -m ai.inference fix broken.adesh
python3 -m ai.inference chat

# Native AdeshLang CLI
adesh ai generate "Create a counter class"
adesh ai explain main.adesh
adesh ai fix broken.adesh
adesh ai chat
```

---

## Editor Integration

The AI assistant provides editor completion, explanation, and auto-fix hooks for `adesh-editor`, VSCode, Helix, and Neovim via stdin/JSON RPC:

```bash
python3 -m ai.src.inference.editor '{"action": "complete", "text": "fn add("}'
```

Output:
```json
{
  "status": "ok",
  "completion": "a: i32, b: i32) -> i32 {\n  return a + b;\n}"
}
```

---

## AdeshBench Metrics

AdeshBench measures:
- **Syntax Accuracy (%)**
- **Compilation Success Rate (%)**
- **Runtime Correctness (%)**
- **API Accuracy (%)**
- **Hallucination Rate (%)**
- **Inference Latency (ms)**
- **Memory Usage (MB)**
