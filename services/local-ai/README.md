# Local AI service foundation

This directory contains a small, independent HTTP service for the future local
transcription, diarization, speaker identity, and meeting-intelligence providers.
The bootstrap implementation is deliberately **mock-only**:

- it does not load or download models;
- it does not import CUDA or GPU libraries;
- it does not call OpenAI, Hugging Face, Ollama, or any other AI endpoint;
- it does not accept provider credentials.

## HTTP contract

| Method | Path            | Bootstrap behavior                                         |
| ------ | --------------- | ---------------------------------------------------------- |
| `GET`  | `/health`       | Reports that the HTTP process is healthy and in mock mode. |
| `GET`  | `/capabilities` | Reports every planned AI capability as unavailable.        |

Responses are JSON and include `Cache-Control: no-store`. Other paths return a
JSON `404`; unsupported methods return a JSON `405`.

## Run locally

Python 3.11 or newer is required. From this directory:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -e ".[dev]"
python -m meeting_intelligence_local_ai
```

The default listener is `127.0.0.1:8091`. The only supported value of
`MI_LOCAL_AI_MODE` is `mock`; startup fails closed if another mode is supplied.

## Check the service

```powershell
Invoke-RestMethod http://127.0.0.1:8091/health
Invoke-RestMethod http://127.0.0.1:8091/capabilities
python -m ruff format --check .
python -m ruff check .
python -m pytest
```

The repository-level `docker-compose.yml` runs the same mock service in a
non-root, read-only container. No credential is present in the image or Compose
configuration.
