# Provider Modes

ScriptScore is designed so exam workflows can run without a required hosted AI service.

## No AI Assistance

No-AI mode keeps model-backed assistance disabled. Workflow tools can still organize, inspect, and export grading artifacts where the selected workflow supports that mode.

## Local Ollama

Local Ollama mode sends model requests to an Ollama service controlled by the user, commonly on the same machine or local network. Users are responsible for the selected model and endpoint.

## Hosted Ollama

Hosted Ollama mode is for user-configured hosted Ollama endpoints. Credentials and endpoint configuration should remain local to the user environment or desktop settings.

## ScriptScorePlus

ScriptScorePlus is planned as a separate hosted API service. The public client may include client-side request types, provider selection values, and disabled or coming-soon UI for that service.

The closed-source hosted service implementation is not included in this repository. ScriptScorePlus is not required for no-AI, local Ollama, or hosted Ollama use.
