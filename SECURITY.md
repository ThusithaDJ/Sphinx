# Security policy

## Reporting a vulnerability

Please **don't open a public issue** for security problems.

Report them privately through GitHub instead: open the repository's
**Security** tab and choose **Report a vulnerability**. Include what you found,
the steps to reproduce it, and which version and operating system you used.

You should get a reply within a week. Once a fix is released, you're welcome
to be credited in the release notes.

## Supported versions

Only the latest release gets security fixes.

## How Sphinx handles your data

- **Everything runs on your machine.** Sphinx has no server and collects no
  telemetry or analytics.
- **Credentials live in your OS credential store**: Windows Credential
  Manager, macOS Keychain, or the Secret Service (GNOME Keyring / KWallet) on
  Linux. That covers SFTP/FTPS passwords and the AI-provider, transcription and
  keyword-API keys. None of them are written to Sphinx's database.
- **The library database** (`sphinx.db`) holds file paths, generated metadata,
  job history and connection details (host, username, folder), but no
  passwords or keys.
- **Network traffic** goes only to the services you configure:
  - The AI provider you pick (OpenAI, Google Gemini, Anthropic) receives the
    images or video keyframes you analyse. Use a local Ollama model to keep
    media on your machine.
  - OpenAI's transcription API receives video audio, if you turn
    transcription on.
  - Shutterstock and Adobe Stock receive keyword searches, if you configure
    keyword enrichment.
  - Your SFTP/FTPS servers receive the files you upload.
- **SFTP host keys** are pinned on the first successful connection. If a
  server's key changes later, Sphinx refuses to connect.
