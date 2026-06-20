## Sync model files from huggingface to cloudflare R2

Make sure you've configured R2 user api token as aws credential.

```SHELL
export R2_ENDPOINT=https://020cfd316d4853132dc053030d7d4653.r2.cloudflarestorage.com
export R2_BUCKET=ariso-app
AWS_PROFILE=r2 ./sync-stt-models.sh
```
