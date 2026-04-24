import React, { useState, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import Chart from "react-apexcharts";
import Layout from "../components/Layout";
import { useAuth } from "../contexts/AuthContext";
import { fetchApi } from "../utils/api";
import "./Dashboard.css";

function Dashboard() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [metrics, setMetrics] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const accessCodesEnabled = user?.features?.access_codes_enabled !== false;

  useEffect(() => {
    fetchMetrics();
  }, []);

  async function fetchMetrics() {
    try {
      const response = await fetchApi("/api/admin/dashboard/metrics");

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
      type: "line",
      height: 350,
      toolbar: {
        show: true,
      },
      zoom: {
        enabled: false,
      },
    },
    stroke: {
      curve: "smooth",
      width: 3,
    },
    colors: ["#3498db"],
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
      align: "left",
      style: {
        fontSize: "18px",
        fontWeight: "600",
        color: "#2c3e50",
      },
    },
    grid: {
      borderColor: "#f1f1f1",
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
      type: "bar",
      height: 350,
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
    plotOptions: {
      bar: {
        borderRadius: 4,
        horizontal: true,
      },
    },
    colors: ["#27ae60"],
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
      align: "left",
      style: {
        fontSize: "18px",
        fontWeight: "600",
        color: "#2c3e50",
      },
    },
    grid: {
      borderColor: "#f1f1f1",
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
        colors: ["#333"],
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
