# Installed runner CLIs and secret redaction

AI Scheduler assumes each configured runner CLI is already installed and authenticated on the machine where the app runs. The app will not manage provider credentials, store API keys, or export environment secrets; run records may store resolved executable paths and expanded argv for debugging, but environment capture must be opt-in and redacted by default. This keeps the app safe to publish publicly without embedding private credentials or machine-specific tokens.
