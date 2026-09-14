# AdeshLang AI (`ai/`)

Lightweight, specialized language model designed exclusively for **AdeshLang v0.3.0**.

## Purpose

The `ai/` project provides a small, highly efficient, extremely accurate language model tailored specifically for:
1. **Understanding AdeshLang syntax & semantics**
2. **Generating correct AdeshLang code**
3. **Completing AdeshLang snippets**
4. **Explaining AdeshLang programs**
5. **Converting pseudocode/requirements into AdeshLang**
6. **Debugging and auto-fixing compiler errors**
7. **Answering stdlib & CLI documentation questions**
8. **Assisting with compiler/runtime architecture**

The model intentionally excludes general-world knowledge to remain compact, fast, and local consumer hardware friendly.

---

## Directory Structure

```text
ai/
├── README.md
├── LICENSE
├── pyproject.toml
├── configs/          # YAML configs for pretraining, finetuning, eval
├── data/             # Raw, curated, processed, synthetic datasets
├── knowledge/        # Structured knowledge base extracted from repository
├── datasets/         # Specialized task datasets (instruction, code, qa, completion, debugging)
├── scripts/          # Command-line entry points
├── src/              # Core modules (dataset, training, evaluation, inference, utils)
├── tests/            # Test suite for AI modules
├── models/           # Checkpoints and exported ONNX/GGUF models
└── notebooks/        # Jupyter notebooks for experimentation
```

---

## Quick Start Commands

```bash
# 1. Collect Knowledge from Codebase
python3 -m ai.dataset.collect

# 2. Curate & Validate Dataset
python3 -m ai.dataset.curate
python3 -m ai.dataset.validate

# 3. Train / Fine-tune Model
python3 -m ai.training.train
python3 -m ai.training.finetune

# 4. Evaluate on AdeshBench
python3 -m ai.evaluation.evaluate

# 5. Interactive Inference / CLI
python3 -m ai.inference generate "Create a counter class"
python3 -m ai.inference explain "examples/hello.adesh"
python3 -m ai.inference fix "broken.adesh"
```

Via `adesh` Rust CLI:
```bash
adesh ai explain file.adesh
adesh ai fix file.adesh
adesh ai generate "Create a binary search function"
adesh ai chat
```

---

## Architecture & Compiler Validation

This project enforces compiler-verified output:

$$\text{Knowledge Base} \longrightarrow \text{Dataset} \longrightarrow \text{Model Output} \longrightarrow \mathbf{\text{AdeshLang Compiler}} \longrightarrow \text{Verified Knowledge}$$

Every synthetic snippet and code output is validated directly against the `adesh` compiler binary whenever possible.
