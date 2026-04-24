import React, { useState } from "react";
import { Link, Navigate } from "react-router-dom";
import { useAuth } from "../contexts/AuthContext";

function Register() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState("");
  const [success, setSuccess] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const { user, register, authConfig } = useAuth();

  const siteDomain = import.meta.env.VITE_SITE_DOMAIN;

  if (user) {
    return <Navigate to="/dashboard" replace />;
  }

  if (authConfig.oidcEnabled) {
    return (
      <div className="container">
        <div className="card">
          <h1>Registration</h1>
          <p>Account registration is managed through Single Sign-On (SSO).</p>
          <p>Please contact your administrator to get access.</p>
          <div className="link">
            <Link to="/login">Back to Login</Link>
          </div>
        </div>
      </div>
    );
  }

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setIsLoading(true);

    if (!email.endsWith(`@${siteDomain}`)) {
      setError(`Email must be from ${siteDomain} domain`);
      setIsLoading(false);
      return;
    }

    // Validate password match
    if (password !== confirmPassword) {
      setError("Passwords do not match");
      setIsLoading(false);
      return;
    }

    // Validate password strength
    if (password.length < 8) {
      setError("Password must be at least 8 characters long");
      setIsLoading(false);
      return;
    }

    try {
      await register(email, password);
      setSuccess(true);
    } catch (err) {
      setError(err.message);
    } finally {
      setIsLoading(false);
    }
  }

  if (success) {
    return (
      <div className="container">
        <div className="card">
          <div className="success">
            <h1>Registration Successful!</h1>
            <p>
              A verification email has been sent to <strong>{email}</strong>.
            </p>
            <p>
              Please check your email and click the verification link to
              activate your account.
            </p>
            <p>The verification link will expire in 24 hours.</p>
          </div>
          <div className="link">
            <Link to="/login">Back to Login</Link>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="container">
      <div className="card">
        <h1>Admin Registration</h1>
        <p>Create a new admin account ({siteDomain} only)</p>

        {error && <div className="error">{error}</div>}

        <form onSubmit={handleSubmit}>
          <div className="form-group">
            <label htmlFor="email">Email</label>
            <input
              type="email"
              id="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              required
              placeholder={`admin@${siteDomain}`}
            />
          </div>

          <div className="form-group">
            <label htmlFor="password">Password</label>
            <input
              type="password"
              id="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
              placeholder="Minimum 8 characters"
            />
          </div>

          <div className="form-group">
            <label htmlFor="confirmPassword">Confirm Password</label>
            <input
              type="password"
              id="confirmPassword"
              value={confirmPassword}
              onChange={(e) => setConfirmPassword(e.target.value)}
              required
              placeholder="Re-enter your password"
            />
          </div>

          <button type="submit" className="btn" disabled={isLoading}>
            {isLoading ? "Registering..." : "Register"}
          </button>
        </form>

        <div className="link">
          Already have an account? <Link to="/login">Login here</Link>
        </div>
      </div>
    </div>
  );
}

export default Register;
