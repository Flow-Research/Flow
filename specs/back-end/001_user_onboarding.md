# User Onboarding and Authentication Specification

**Version**: v1.0 - September 24, 2025
**Author**: [Allwell Agwu-Okoro](https://github.com/allwelldotdev/)
**Status**: Draft - Review by [Julian Duru](https://github.com/durutheguru)

## Introduction
This spec outlines secure user onboarding for Flow. Core principles: Offline-first access, user sovereignty over data, optional sync to decentralized network/remote (cloud) backups.

## Goals
- Enable password-based or passphrase-based local auth.
  - **password-based**: Password is typed. User inputs password upon offline-first access.
  - **passphrase-based**: Passphrase is pre-generated. User is presented with pre-generated passphrase upon offline-first access, and asked to export passphrase offline.
- Support passwordless decentralized network access via WebAuthn (passkeys).
- Integrate Privy for seamless web3 social/wallet onboarding.

## Components
- IDs
  - **Device ID**: Tied to a user's device (one/many per user).
  - **Network ID**: Tied to a user's network DID (one per user).
    - Enables remote (cloud) data backup.
    - Enables blockchain wallet.
    - Is user's root DID across the network. Root DID is parent node to one or more device ID sibling node(s).
- Local Auth (Offline-first)
  - Method: Master password or passphrase -> Argon2id KDF (via `argon2` crate) -> AES-256-GCM key to securely encrypt local data storage (via `aes-gcm`)
  - Recovery:
    - No recovery. Improved security because one possible master key, no resets, plus data encrypted at rest.
    - User is advised to back up their password via UX.
    - Incentive to access network to gain network ID and enable remote data backup.
  - Rust Crates: `argon2`, `aes-gcm`.
- Network Auth (Decentralized Network)
  - Method: WebAuthn (passkey) registration and authentication ceremony.
  - Implementation: Server-side RP with `webauthn-rs`.
  - Fallback: Privy or password.
  - Recovery: 
    - Wallet recovery via BIP39 mnemonic codes (12-24 word recovery phrase via `bip39` crate). User advised to back up recovery phrase via UX.
  - Rust Crates: `webauthn-rs`, `bip39`.
- Privy Layer
  - Role: OAuth2.0-like flows for web3 social and wallet management as option or fallback for WebAuthn (passkeys).
  - Integration: API calls via `reqwest`; handle JWTs for sessions.
  - Config: App ID from Privy dashboard to access Privy's services. Enable web3 socials, email-based OTP and/or passkey providers.

## Flows
> *Work in progress...*

## Security and Privacy

### Threat Model
- Adversary: Device theft, MITM on sync, phishing for keys.
- Mitigations: Full-disk encryption locked by master key. Zero-knowledge proofs for sync verification.

### Risks
- Password weakness: Enforce 12+ chars via `zxcvbn` scoring.
- Privy dependency: Fallback to direct WebAuthn or password if API down or cost-concern.

## Open Questions
- For seamless offline-first user onboarding, should we offer one option between an inputted "password" or pre-generated "passphrase"?
- Local-data export:
  - Should users be allowed to export their data from one device to another, in a fungible data format, instead of using a remote backup option or accessing the network?
  - Should users be allowed to choose if their data should be exported securely (encrypted at rest) or otherwise (in cleartext)?

