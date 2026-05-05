# Privacy And Data

ScriptScore is intended for sensitive exam workflows. Treat all exam files, student identifiers, grading traces, and local project databases as private data.

## Do Not Commit Sensitive Data

Do not commit:

- Real student names, emails, IDs, or LMS identifiers.
- Canvas or LMS credentials.
- API keys or provider tokens.
- Raw PDFs, raw scans, or screenshots containing real submissions.
- Generated traces or local project databases.

Test fixtures in this repository must be synthetic and safe to publish.

## Local Workflows

No-AI and local Ollama workflows are intended to avoid a required ScriptScore-hosted service. Local Ollama requests are sent to the endpoint configured by the user.

## Hosted Endpoints

Hosted Ollama and future ScriptScorePlus use involve network requests to configured hosted services. Users should review provider terms, privacy expectations, and institutional policies before sending exam content to hosted endpoints.

## Redaction

Some workflows support redaction and PII prescreening, but these features should be treated as workflow aids rather than a guarantee that all sensitive data has been removed. Review outputs before sharing them outside the intended grading context.
