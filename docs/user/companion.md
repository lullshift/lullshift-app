# Companion

Companion is an optional chat with an AI provider, using your own API key (BYOK: bring your own key). It is a separate workspace. It can only reply with text. It can't run commands or act on your agents.

Lullshift has no hosted AI. Until you add a provider, Companion sends nothing.

## Adding a provider

Open **Companion**, then **Settings**, then **Add a provider**.

| Provider type | Where messages go |
|---|---|
| Anthropic | `https://api.anthropic.com` |
| OpenAI | `https://api.openai.com/v1` |
| OpenAI-compatible | The base URL you enter, including the version part, for example `https://openrouter.ai/api/v1` or `http://localhost:11434/v1`. Plain `http` is allowed only for `localhost`, `127.0.0.1` and `::1` |

Give it a name, a model and, if you like, a monthly token budget. Companion shows "Sends to" with the host name, so you always know where your text goes.

## Your key

- Choose **Paste key**. The key goes straight into your system keychain (Windows Credential Manager, macOS Keychain or the Linux Secret Service) and is never shown again.
- The key is never stored in Lullshift's database or logs.
- If the keychain can't be used (for example, Linux without a Secret Service), Companion says "Keychain unavailable" and doesn't store the key anywhere else.
- **Test connection** sends one tiny request, only when you press it.
- Removing a provider also deletes its key from the keychain.

## Choosing what to share

When you write a message, you choose the context to share with it:

| Option | What is sent with your message |
|---|---|
| No context | Only the words you type |
| Session summary | One session's title, status, intent and latest ask |
| Selected notes | Only the notes and checklist items you tick from one session |

Before sending with context, you see a preview of exactly what will be attached. If the context changes after you previewed it, Lullshift asks you to review it again.

Context goes only with the message you chose it for. Earlier messages are sent as history only to the same host they went to before.

## Budget and errors

- A monthly token budget stops sending once this month's usage reaches it. A message that starts under the limit may end a little over it.
- If you are offline, rate-limited, or the key is rejected, the message is kept as failed and you can retry it.
- **Cancel** returns at once, but the request may already have reached the provider.

## Data

Conversations are stored only on your computer. Your provider's own terms apply to what you send them.
