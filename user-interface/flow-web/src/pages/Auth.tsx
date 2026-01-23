import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '../contexts/AuthContext';
import { api } from '../services/api';
import './Auth.css';

function base64UrlToBuffer(base64url: string): ArrayBuffer {
  const base64 = base64url.replace(/-/g, '+').replace(/_/g, '/');
  const padded = base64.padEnd(base64.length + (4 - (base64.length % 4)) % 4, '=');
  const binary = atob(padded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}

function bufferToBase64Url(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  const base64 = btoa(binary);
  return base64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}

export function AuthPage() {
  const { login } = useAuth();
  const navigate = useNavigate();
  const [isRegistering, setIsRegistering] = useState(false);
  const [isAuthenticating, setIsAuthenticating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const handleRegister = async () => {
    setError(null);
    setSuccess(null);
    setIsRegistering(true);

    try {
      const { challenge, challenge_id } = await api.auth.startRegistration();
      const publicKey = (challenge as unknown as { publicKey: PublicKeyCredentialCreationOptions }).publicKey;

      const options: PublicKeyCredentialCreationOptions = {
        challenge: base64UrlToBuffer(publicKey.challenge as unknown as string),
        rp: publicKey.rp,
        user: {
          id: base64UrlToBuffer((publicKey.user.id as unknown as string)),
          name: publicKey.user.name,
          displayName: publicKey.user.displayName,
        },
        pubKeyCredParams: publicKey.pubKeyCredParams,
        timeout: publicKey.timeout,
        authenticatorSelection: publicKey.authenticatorSelection,
        attestation: publicKey.attestation,
      };

      const credential = await navigator.credentials.create({ publicKey: options }) as PublicKeyCredential;
      if (!credential) throw new Error('Failed to create credential');

      const attestationResponse = credential.response as AuthenticatorAttestationResponse;
      const credentialJSON = {
        id: credential.id,
        rawId: bufferToBase64Url(credential.rawId),
        response: {
          attestationObject: bufferToBase64Url(attestationResponse.attestationObject),
          clientDataJSON: bufferToBase64Url(attestationResponse.clientDataJSON),
        },
        type: credential.type,
        extensions: credential.getClientExtensionResults(),
      };

      const result = await api.auth.finishRegistration(challenge_id, credentialJSON);
      
      if (result.verified) {
        setSuccess('Registration successful! You can now authenticate.');
      } else {
        throw new Error('Registration verification failed');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Registration failed');
    } finally {
      setIsRegistering(false);
    }
  };

  const handleAuthenticate = async () => {
    setError(null);
    setSuccess(null);
    setIsAuthenticating(true);

    try {
      const { challenge, challenge_id } = await api.auth.startAuthentication();
      const publicKey = (challenge as unknown as { publicKey: PublicKeyCredentialRequestOptions }).publicKey;

      const options: PublicKeyCredentialRequestOptions = {
        challenge: base64UrlToBuffer(publicKey.challenge as unknown as string),
        rpId: publicKey.rpId,
        timeout: publicKey.timeout,
        userVerification: publicKey.userVerification,
        allowCredentials: (publicKey.allowCredentials as unknown as Array<{ id: string; type: string; transports?: string[] }>)?.map((cred) => ({
          id: base64UrlToBuffer(cred.id),
          type: cred.type as PublicKeyCredentialType,
          transports: cred.transports as AuthenticatorTransport[],
        })),
      };

      const credential = await navigator.credentials.get({ publicKey: options }) as PublicKeyCredential;
      if (!credential) throw new Error('Failed to get credential');

      const assertionResponse = credential.response as AuthenticatorAssertionResponse;
      const credentialJSON = {
        id: credential.id,
        rawId: bufferToBase64Url(credential.rawId),
        response: {
          authenticatorData: bufferToBase64Url(assertionResponse.authenticatorData),
          clientDataJSON: bufferToBase64Url(assertionResponse.clientDataJSON),
          signature: bufferToBase64Url(assertionResponse.signature),
          userHandle: assertionResponse.userHandle ? bufferToBase64Url(assertionResponse.userHandle) : null,
        },
        type: credential.type,
      };

      const result = await api.auth.finishAuthentication(challenge_id, credentialJSON);
      
      if (result.verified && result.token && result.did) {
        login(result.token, result.did);
        navigate('/');
      } else {
        throw new Error('Authentication verification failed');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Authentication failed');
    } finally {
      setIsAuthenticating(false);
    }
  };

  return (
    <div className="auth-page">
      <div className="auth-container">
        <div className="auth-header">
          <h1>Welcome to Flow</h1>
          <p>Sign in with your passkey to continue</p>
        </div>

        <div className="auth-buttons">
          <button
            onClick={handleAuthenticate}
            disabled={isAuthenticating || isRegistering}
            className="auth-btn primary"
          >
            {isAuthenticating ? 'Authenticating...' : 'Sign In with Passkey'}
          </button>

          <div className="auth-divider">
            <span>or</span>
          </div>

          <button
            onClick={handleRegister}
            disabled={isRegistering || isAuthenticating}
            className="auth-btn secondary"
          >
            {isRegistering ? 'Registering...' : 'Create New Account'}
          </button>
        </div>

        {error && <div className="auth-error">{error}</div>}
        {success && <div className="auth-success">{success}</div>}

        <div className="auth-info">
          <p>Flow uses passkeys for secure, passwordless authentication.</p>
          <p>Your passkey is stored securely on your device.</p>
        </div>
      </div>
    </div>
  );
}
