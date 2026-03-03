# 🐕 Lares: The Only Agent That Actually Knows Where Your Config Is.

**Stop babysitting your computer. Lares is the system management daemon that fixes your OS while you grab coffee. It’s declarative, it’s rollback-native, and it’s tired of your messy dotfiles.**

---

## 💀 The Status Quo Sucks

Computers are tools, but lately, they feel like high-maintenance pets.

*   **Apple/Microsoft** think you're too incompetent to touch the settings. They’ve locked you in a gilded cage where "user-friendly" means "you do it our way or not at all."
*   **Linux** gives you the keys to the kingdom, but the kingdom is currently on fire because you tried to update your GPU drivers at 2 AM. 

You’ve been stuck choosing between a **walled garden** and a **landmine field**. We’re here to give you the third option: a system that actually adapts to *you*, without requiring a PhD in shell scripting.

## ⚡️ The Secret Sauce: AI + Nix

Most "AI agents" are just glorified chatbots that hallucinate shell commands. If they mess up, your system is bricked. Good luck with that.

**Lares is different because it uses Nix.** 

By forcing the agent to work through a declarative, functional configuration layer, we’ve turned "dangerous autonomy" into "safe automation."
*   **Atomic Rebuilds:** It either works, or it fails silently. No half-installed packages or corrupted configs.
*   **The Big Red Button:** If the agent does something you don't like, just say `"undo that"`. Lares reverts the git commit, rolls back the Nix generation, and acts like nothing ever happened.
*   **Total Audit Trail:** Every single change is a signed git commit. You can see exactly what the agent was thinking, what it changed, and why. No more "who changed my wallpaper?" mysteries.

## 🛠 What can it actually do?

Lares lives in your system as a root-owned daemon (safety first, kids). You talk to it, and it gets to work.

> **"Make my terminal look like a 1980s hacker movie."**
> *Lares finds the right Nix modules, sets the fonts, configures the transparency, and rebuilds your shell. Your CPU barely noticed.*

> **"I'm going to a coffee shop. Block all incoming traffic except SSH and turn on the VPN if I'm on public Wi-Fi."**
> *Lares updates your networking services, commits the security policy, and sets up a log-watcher to keep you safe.*

> **"Everything feels slow today."**
> *Lares doesn't just run `top`. It checks your background services, correlates CPU spikes with recent Nix updates, identifies a leaking daemon, and offers to kill/revert it.*

---

## 🚩 The "Fine Print" (Read this)

We aren't selling snake oil.

1.  **Nix isn't optional.** If you don't like Nix, you won't like Lares. We use it as a shield to keep the agent from hurting your system.
2.  **macOS is a Guest.** On NixOS, we're the king. On macOS, we're fighting Apple's "SIP" and proprietary Plists. We can handle your packages and dev environment, but we can't fix your hardware firmware (yet).
3.  **Alpha as Hell.** This is a CLI-first tool for people who like living on the edge. If you want a shiny "Next" button, check back in six months.

## 🚀 Dive In (If You Dare)

If you're tired of being your laptop's unpaid intern, start here:

*   **[Getting Started](./docs/getting-started.md)**: How to let the wolf into the house.
*   **[Configuration](./docs/config.md)**: Options for the daemon and API.
*   **[Architecture](./docs/architecture.md)**: How we keep the LLM from burning everything down.
*   **[Nix Strategy](./docs/nix-strategy.md)**: Why declarative config is the only way to live.
*   **[Security](./docs/security.md)**: Trust tiers, privilege escalation, and how we stay safe.

**The future of computing is conversational, declarative, and actually useful.**

---
*Lares: Your computer, but smarter than you for once.*
