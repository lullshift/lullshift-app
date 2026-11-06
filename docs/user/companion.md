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

The next message starts with the choice you used last, so you don't have to pick it again. You still see the exact text in a preview, and confirm it, before every send. If the session you used last is gone, the composer goes back to No context and tells you.

Context goes only with the message you chose it for. Earlier messages are sent as history only to the same host they went to before.

## Replies appear as they are written

Replies stream in as the provider writes them, as plain text with no links. A screen reader hears a short status ("Companion is replying") and then the finished reply once, not every word. If the connection drops part-way, the message is marked as not sent and you can try again; the part that arrived still counts toward your budget.

## Lookups (off by default)

In **Settings**, **Let the Companion look things up** lets the provider ask for three read-only lookups:

| Lookup | What it returns |
|---|---|
| Session overview | Your agent sessions: tool, state and how long it has been in that state. A session label only for a tool where you allowed task text: the name you gave it, the agent's title or, if neither exists, the first line of the captured prompt (cut short). Never notes, files or session ids |
| Suggest a break | One installed activity and about how long it takes. You get a card with a **Start** button; nothing starts unless you press it |
| Activity help | The how-to-play help of one installed activity. This text is written by the activity's author |

Every lookup is shown to you with the exact text that would be sent, and nothing goes to the provider until you press **Approve**. **Decline** sends only "The user declined." A message can use at most three lookups. Lookups can't change anything, open anything or run anything; the Companion still can only reply with text. They add a little to each request and count toward your token budget. Some servers don't support them: turn the setting off if replies fail.

If the provider asks for something Lullshift won't give (a lookup that doesn't exist, one with arguments that aren't valid, or more lookups than one message allows), you see a small notice in the conversation, for example "The Companion asked for something it can't have (an unknown lookup). Nothing was sent." There is nothing to approve. The notice names the lookup only if its name is short and plain, and never shows what the provider put in its request. It stays in the conversation history with the message.

## Budget and errors

- A monthly token budget stops sending once this month's usage reaches it. A message that starts under the limit may end a little over it. If the Companion uses lookups, the budget is checked again before each follow-up call.
- If you are offline, rate-limited, or the key is rejected, the message is kept as failed and you can retry it.
- If a provider goes quiet for 30 seconds mid-reply (or 60 seconds before it starts), Lullshift gives up on that reply.
- **Cancel** stops the reply at once and you can write again straight away. Anything the provider still sends for it is thrown away. The request may already have reached the provider, so it still counts toward your usage.

## Data

Conversations are stored only on your computer. Your provider's own terms apply to what you send them.
