import React, { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { fetchApi } from '../utils/api';

function VerifyEmail() {
  const [searchParams] = useSearchParams();
  const [status, setStatus] = useState('verifying');
  const [error, setError] = useState('');
  const [email, setEmail] = useState('');

  useEffect(() => {
    verifyEmail();
  }, []);

  async function verifyEmail() {
    const token = searchParams.get('token');

    if (!token) {
      setStatus('error');
      setError('No verification token provided');
      return;
    }

    try {
      const response = await fetchApi(`/api/admin/verify-email?token=${token}`, {
        method: 'GET',
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || 'Verification failed');
      }

      const data = await response.json();
      setEmail(data.email);
      setStatus('success');
    } catch (err) {
      setStatus('error');
      setError(err.message);
    }
  }

  if (status === 'verifying') {
    return (
      <div className="container">
        <div className="card">
          <h1>Verifying Email...</h1>
          <p>Please wait while we verify your email address.</p>
        </div>
      </div>
    );
  }

  if (status === 'error') {
    return (
      <div className="container">
        <div className="card">
          <div className="error">
            <h1>Verification Failed</h1>
            <p>{error}</p>
          </div>
          <div className="link">
            <Link to="/register">Register Again</Link> or{' '}
            <Link to="/login">Login</Link>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="container">
      <div className="card">
        <div className="success">
          <h1>Email Verified!</h1>
          <p>
            Your email <strong>{email}</strong> has been successfully verified.
          </p>
          <p>You can now log in to your admin account.</p>
        </div>
        <div className="link">
          <Link to="/login">Go to Login</Link>
        </div>
      </div>
    </div>
  );
}

export default VerifyEmail;
