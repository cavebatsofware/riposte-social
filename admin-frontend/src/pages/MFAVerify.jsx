import React, { useState } from 'react';
import { Navigate } from 'react-router-dom';
import { useAuth } from '../contexts/AuthContext';
import './MFAVerify.css';

function MFAVerify() {
  const [code, setCode] = useState('');
  const [error, setError] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const { user, verifyMFA, logout } = useAuth();

  // If not logged in at all, redirect to login
  if (!user) {
    return <Navigate to="/login" replace />;
  }

  // If MFA is not required, redirect to dashboard
  if (!user.mfa_required) {
    return <Navigate to="/dashboard" replace />;
  }

  async function handleSubmit(e) {
    e.preventDefault();
    setError('');
    setIsLoading(true);

    try {
      await verifyMFA(code);
    } catch (err) {
      setError(err.message);
      setCode('');
    } finally {
      setIsLoading(false);
    }
  }

  function handleCodeChange(e) {
    const value = e.target.value.replace(/\D/g, '').slice(0, 6);
    setCode(value);
  }

  async function handleCancel() {
    await logout();
  }

  return (
    <div className="mfa-verify-container">
      <div className="mfa-verify-card card">
        <div className="mfa-verify-icon">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor" width="48" height="48">
            <path d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 10.99h7c-.53 4.12-3.28 7.79-7 8.94V12H5V6.3l7-3.11v8.8z"/>
          </svg>
        </div>
        <h1>Two-Factor Authentication</h1>
        <p>Enter the 6-digit code from your authenticator app</p>

        {error && <div className="error">{error}</div>}

        <form onSubmit={handleSubmit}>
          <div className="form-group">
            <input
              type="text"
              inputMode="numeric"
              autoComplete="one-time-code"
              value={code}
              onChange={handleCodeChange}
              placeholder="000000"
              className="mfa-code-input"
              maxLength="6"
              autoFocus
              required
            />
          </div>

          <div className="mfa-verify-actions">
            <button type="submit" className="btn-primary" disabled={isLoading || code.length !== 6}>
              {isLoading ? 'Verifying...' : 'Verify'}
            </button>
            <button type="button" className="btn-secondary" onClick={handleCancel}>
              Cancel
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

export default MFAVerify;
