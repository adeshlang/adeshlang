1. Upload Merged PyTorch / SafeTensors Model (merged-latest)
bash


python ai/scripts/upload_to_hf.py \
  --path "ai/models/merged-latest" \
  --repo-id "adeshlang/adesh-coder-0.5b" \
  --commit-message "Upload merged safetensors model weights"
  
2. Upload GGUF Quantized Weights
bash


python ai/scripts/upload_to_hf.py \
  --path "ai/models/adesh-coder-0.5b-q4_0.gguf" \
  --repo-id "adeshlang/adesh-coder-0.5b" \
  --commit-message "Upload Q4_0 GGUF quantized model"
bash


python ai/scripts/upload_to_hf.py \
  --path "ai/models/adesh-coder-0.5b-q8_0.gguf" \
  --repo-id "adeshlang/adesh-coder-0.5b" \
  --commit-message "Upload Q8_0 GGUF quantized model"
3. Upload Entire models/ Directory (All Checkpoints + GGUF + Adapters)
bash


python ai/scripts/upload_to_hf.py \
  --path "ai/models" \
  --repo-id "adeshlang/adesh-coder-0.5b" \
  --commit-message "Upload full model bundle"