import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSiteConfig } from "../../contexts/SiteConfigContext";
import { fetchApi } from "../../utils/api";
import Turnstile from "./Turnstile";
import "./Contact.css";

/// `/contact`: public contact form, reachable without an account or a
/// purchase. Posts name/email/subject/message to `POST /api/contact` (the
/// social server's CSRF-gated route, so `fetchApi` attaches the token). A
/// Turnstile token is included when the operator has configured a site key.
export default function Contact() {
  const { t } = useTranslation("contact");
  const { config: site } = useSiteConfig();
  const siteKey = site?.turnstile_site_key as string | undefined;

  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [subject, setSubject] = useState("");
  const [message, setMessage] = useState("");
  const [token, setToken] = useState<string | null>(null);

  const [sending, setSending] = useState(false);
  const [error, setError] = useState("");
  const [sent, setSent] = useState(false);

  const onToken = useCallback((value: string | null) => setToken(value), []);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSending(true);
    setError("");
    try {
      const response = await fetchApi("/api/contact", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name,
          email,
          subject,
          message,
          turnstile_token: token,
        }),
      });
      const data = await response.json().catch(() => null);
      if (response.ok && data?.success) {
        setSent(true);
      } else if (response.status === 429) {
        // A recent submission is rejected with 429; say the first message went
        // through instead of surfacing a terse server string or a generic error.
        setError(t("rateLimited"));
      } else {
        setError(data?.message || t("error"));
      }
    } catch {
      setError(t("error"));
    } finally {
      setSending(false);
    }
  }

  if (sent) {
    return (
      <div className="contact-page">
        <h1 className="contact-title">{t("title")}</h1>
        <div className="contact-success" role="status">
          {t("success")}
        </div>
      </div>
    );
  }

  return (
    <div className="contact-page">
      <h1 className="contact-title">{t("title")}</h1>
      <p className="contact-intro">{t("intro")}</p>
      {error && (
        <div className="contact-error" role="alert">
          {error}
        </div>
      )}
      <form className="contact-form" aria-label={t("title")} onSubmit={handleSubmit}>
        <div className="contact-field">
          <label className="contact-label" htmlFor="contact-name">
            {t("fields.name")}{" "}
            <span className="contact-required" aria-hidden="true">
              *
            </span>
          </label>
          {/* eslint-disable-next-line a11yinspect/required-element-warning -- rule fires unconditionally; asterisk in label above is the visual indicator */}
          <input
            id="contact-name"
            name="name"
            type="text"
            autoComplete="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={100}
            required
            aria-required="true"
          />
        </div>
        <div className="contact-field">
          <label className="contact-label" htmlFor="contact-email">
            {t("fields.email")}{" "}
            <span className="contact-required" aria-hidden="true">
              *
            </span>
          </label>
          {/* eslint-disable-next-line a11yinspect/required-element-warning -- rule fires unconditionally; asterisk in label above is the visual indicator */}
          <input
            id="contact-email"
            name="email"
            type="email"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            maxLength={254}
            required
            aria-required="true"
          />
        </div>
        <div className="contact-field">
          <label className="contact-label" htmlFor="contact-subject">
            {t("fields.subject")}{" "}
            <span className="contact-required" aria-hidden="true">
              *
            </span>
          </label>
          {/* eslint-disable-next-line a11yinspect/required-element-warning -- rule fires unconditionally; asterisk in label above is the visual indicator */}
          <input
            id="contact-subject"
            name="subject"
            type="text"
            autoComplete="off"
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
            maxLength={200}
            required
            aria-required="true"
          />
        </div>
        <div className="contact-field">
          <label className="contact-label" htmlFor="contact-message">
            {t("fields.message")}{" "}
            <span className="contact-required" aria-hidden="true">
              *
            </span>
          </label>
          {/* eslint-disable-next-line a11yinspect/required-element-warning -- rule fires unconditionally; asterisk in label above is the visual indicator */}
          <textarea
            id="contact-message"
            name="message"
            autoComplete="off"
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            maxLength={5000}
            rows={6}
            required
            aria-required="true"
          />
        </div>
        <Turnstile siteKey={siteKey} onToken={onToken} className="contact-turnstile" />
        <button
          type="submit"
          className="btn-primary contact-submit"
          disabled={sending}
        >
          {sending ? t("sending") : t("submit")}
        </button>
      </form>
    </div>
  );
}
