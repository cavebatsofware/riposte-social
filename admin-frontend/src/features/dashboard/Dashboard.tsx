import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import Chart from "react-apexcharts";
import { useTheme } from "@cavebatsofware/riposte-design-system/theme";
import Layout from "../../components/Layout";
import { useAuth } from "../../contexts/AuthContext";
import { fetchDashboardMetrics } from "./api";
import "./Dashboard.css";

/// ApexCharts is configured in JS, not CSS, so it can't read design tokens by
/// itself. Resolve the current colorway's tokens off <html> at runtime; the
/// component re-reads these whenever the theme changes so the charts retrace in
/// the active colorway. Fallbacks are the pre-theme hardcoded values.
function readChartTokens() {
  const fallback = {
    primary: "#3498db",
    accent: "#27ae60",
    text: "#2c3e50",
    muted: "#6b7280",
    border: "#f1f1f1",
    onPrimary: "#ffffff",
  };
  if (typeof window === "undefined") return fallback;
  const s = getComputedStyle(document.documentElement);
  const get = (name: string, fb: string) =>
    s.getPropertyValue(name).trim() || fb;
  return {
    primary: get("--color-primary", fallback.primary),
    accent: get("--color-success", fallback.accent),
    text: get("--color-text", fallback.text),
    muted: get("--color-muted", fallback.muted),
    border: get("--color-border", fallback.border),
    onPrimary: get("--color-on-primary", fallback.onPrimary),
  };
}

function Dashboard() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const { theme, mode } = useTheme();
  const [metrics, setMetrics] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const accessCodesEnabled = user?.features?.access_codes_enabled !== false;

  // Re-read tokens after each theme change. A passive effect runs after the
  // ThemeProvider's layout effect has applied the new `data-theme`, so
  // getComputedStyle sees the active colorway's values.
  const [tk, setTk] = useState(readChartTokens);
  useEffect(() => {
    setTk(readChartTokens());
  }, [theme]);

  useEffect(() => {
    fetchMetrics();
  }, []);

  async function fetchMetrics() {
    try {
      const response = await fetchDashboardMetrics();

      if (!response.ok) {
        throw new Error("Failed to fetch dashboard metrics");
      }

      const data = await response.json();
      setMetrics(data);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  // Line chart configuration for hourly access rates
  const accessRateChartOptions = {
    chart: {
      type: "line" as const,
      height: 350,
      foreColor: tk.muted,
      background: "transparent",
      toolbar: {
        show: true,
      },
      zoom: {
        enabled: false,
      },
    },
    theme: { mode },
    stroke: {
      curve: "smooth" as const,
      width: 3,
    },
    colors: [tk.primary],
    xaxis: {
      categories: metrics?.hourly_access_rates?.map((d) => d.hour) || [],
      title: {
        text: "Time (Last 24 Hours)",
      },
      labels: {
        rotate: -45,
        rotateAlways: true,
      },
    },
    yaxis: {
      title: {
        text: "Access Count",
      },
      forceNiceScale: true,
      min: 0,
    },
    title: {
      text: "Access Rate (Last 24 Hours)",
      align: "left" as const,
      style: {
        fontSize: "18px",
        fontWeight: "600",
        color: tk.text,
      },
    },
    grid: {
      borderColor: tk.border,
    },
    tooltip: {
      y: {
        formatter: (value) => `${value} accesses`,
      },
    },
  };

  const accessRateChartSeries = [
    {
      name: "Accesses",
      data: metrics?.hourly_access_rates?.map((d) => d.count) || [],
    },
  ];

  // Bar chart configuration for recent access codes
  const recentCodesChartOptions = {
    chart: {
      type: "bar" as const,
      height: 350,
      foreColor: tk.muted,
      background: "transparent",
      toolbar: {
        show: true,
      },
      events: {
        dataPointSelection: (event, chartContext, config) => {
          const codeId =
            metrics?.recent_access_codes?.[config.dataPointIndex]?.id;
          if (codeId) {
            navigate(`/access-codes?highlight=${codeId}`);
          }
        },
      },
    },
    theme: { mode },
    plotOptions: {
      bar: {
        borderRadius: 4,
        horizontal: true,
      },
    },
    colors: [tk.accent],
    xaxis: {
      categories: metrics?.recent_access_codes?.map((d) => d.name) || [],
      title: {
        text: "Usage Count",
      },
    },
    yaxis: {
      title: {
        text: "Access Code",
      },
    },
    title: {
      text: "Most Used Access Codes (Last 24 Hours)",
      align: "left" as const,
      style: {
        fontSize: "18px",
        fontWeight: "600",
        color: tk.text,
      },
    },
    grid: {
      borderColor: tk.border,
    },
    tooltip: {
      y: {
        formatter: (value) => `${value} uses`,
      },
    },
    dataLabels: {
      enabled: true,
      style: {
        fontSize: "12px",
        colors: [tk.onPrimary],
      },
      background: {
        enabled: false,
      },
    },
  };

  const recentCodesChartSeries = [
    {
      name: "Usage",
      data: metrics?.recent_access_codes?.map((d) => d.count) || [],
    },
  ];

  if (loading) {
    return (
      <Layout>
        <div className="dashboard-content">
          <div className="loading">Loading dashboard...</div>
        </div>
      </Layout>
    );
  }

  return (
    <Layout>
      <div className="dashboard-content">
        <div className="welcome-card">
          <h2>Dashboard</h2>
          <p>Overview of access patterns and usage statistics.</p>
        </div>

        {error && <div className="error">{error}</div>}

        {metrics && (
          <div className="metrics-section">
            <div className="chart-container">
              <Chart
                options={accessRateChartOptions}
                series={accessRateChartSeries}
                type="line"
                height={350}
              />
            </div>

            {accessCodesEnabled && (
              <div className="chart-container">
                <Chart
                  options={recentCodesChartOptions}
                  series={recentCodesChartSeries}
                  type="bar"
                  height={350}
                />
              </div>
            )}
          </div>
        )}

        <div className="feature-grid">
          {accessCodesEnabled && (
            <div className="feature-card">
              <h3>Access Codes</h3>
              <p>Manage and edit access codes for the site.</p>
              <button
                className="btn-feature"
                onClick={() => navigate("/access-codes")}
              >
                Manage Codes
              </button>
            </div>
          )}

          <div className="feature-card">
            <h3>Access Logs</h3>
            <p>View access logs and usage statistics.</p>
            <button
              className="btn-feature"
              onClick={() => navigate("/access-logs")}
            >
              View Logs
            </button>
          </div>

          <div className="feature-card">
            <h3>Settings</h3>
            <p>Configure site settings and preferences.</p>
            <button
              className="btn-feature"
              onClick={() => navigate("/settings")}
            >
              Manage Settings
            </button>
          </div>
        </div>
      </div>
    </Layout>
  );
}

export default Dashboard;
