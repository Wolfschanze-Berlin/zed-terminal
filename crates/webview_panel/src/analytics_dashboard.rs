/// The embedded HTML/CSS/JS for the analytics dashboard.
/// This is the PoC that validates the webview+IPC architecture end-to-end.
/// Uses --zed-* CSS variables for theme matching, and the IPC bridge to
/// request session data from the Rust host.
///
/// Security note: This HTML runs inside a sandboxed WebView2 process with
/// navigation locked and no external script loading. All data is generated
/// locally from the host process via IPC. innerHTML is used for rendering
/// because all content is host-generated (no user-supplied HTML).
pub const ANALYTICS_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: system-ui, -apple-system, sans-serif;
    background: var(--zed-panel-background, #1e1e2e);
    color: var(--zed-text, #cdd6f4);
    padding: 16px;
    overflow-y: auto;
    min-height: 100vh;
  }
  h1 { font-size: 18px; font-weight: 600; margin-bottom: 4px; }
  .subtitle { color: var(--zed-text-muted, #6c7086); font-size: 12px; margin-bottom: 16px; }

  .metrics-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 8px;
    margin-bottom: 20px;
  }
  .metric-card {
    background: var(--zed-surface-background, #24243e);
    border: 1px solid var(--zed-border, #313244);
    border-radius: 6px;
    padding: 12px;
  }
  .metric-value {
    font-size: 22px;
    font-weight: 700;
    color: var(--zed-text-accent, #89b4fa);
  }
  .metric-label {
    font-size: 11px;
    color: var(--zed-text-muted, #6c7086);
    margin-top: 2px;
  }

  .section { margin-bottom: 20px; }
  .section-title {
    font-size: 13px;
    font-weight: 600;
    margin-bottom: 8px;
    color: var(--zed-text, #cdd6f4);
  }

  .bar-chart { display: flex; align-items: flex-end; gap: 4px; height: 120px; }
  .bar-col { display: flex; flex-direction: column; align-items: center; flex: 1; height: 100%; justify-content: flex-end; }
  .bar {
    width: 100%;
    max-width: 32px;
    background: var(--zed-text-accent, #89b4fa);
    border-radius: 3px 3px 0 0;
    min-height: 2px;
    transition: height 0.3s ease;
  }
  .bar-label { font-size: 9px; color: var(--zed-text-muted, #6c7086); margin-top: 4px; }

  .heatmap { display: flex; gap: 2px; flex-wrap: wrap; }
  .heat-cell {
    width: 12px; height: 12px;
    border-radius: 2px;
    background: var(--zed-element-background, #313244);
  }
  .heat-1 { background: rgba(137, 180, 250, 0.2); }
  .heat-2 { background: rgba(137, 180, 250, 0.4); }
  .heat-3 { background: rgba(137, 180, 250, 0.6); }
  .heat-4 { background: rgba(137, 180, 250, 0.85); }

  .donut-container { display: flex; align-items: center; gap: 16px; }
  .donut-svg { width: 80px; height: 80px; }
  .donut-legend { display: flex; flex-direction: column; gap: 4px; }
  .legend-item { display: flex; align-items: center; gap: 6px; font-size: 11px; }
  .legend-dot { width: 8px; height: 8px; border-radius: 50%; }

  .data-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .data-table th {
    text-align: left;
    color: var(--zed-text-muted, #6c7086);
    font-weight: 500;
    padding: 6px 8px;
    border-bottom: 1px solid var(--zed-border, #313244);
  }
  .data-table td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--zed-border-variant, #313244);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 300px;
    color: var(--zed-text-muted, #6c7086);
    text-align: center;
  }
  .empty-state h2 { font-size: 16px; margin-bottom: 8px; color: var(--zed-text, #cdd6f4); }
  .empty-state p { font-size: 12px; max-width: 300px; line-height: 1.5; }
  .empty-state code {
    background: var(--zed-element-background, #313244);
    padding: 2px 6px;
    border-radius: 3px;
    font-size: 11px;
  }

  .loading { text-align: center; padding: 40px; color: var(--zed-text-muted); }
</style>
</head>
<body>
<div id="app">
  <div class="loading">Loading analytics data...</div>
</div>

<script>
(function() {
  'use strict';

  function waitForBridge(callback) {
    if (window.__zed_ipc) { callback(); }
    else { setTimeout(function() { waitForBridge(callback); }, 50); }
  }

  function el(tag, attrs, children) {
    var node = document.createElement(tag);
    if (attrs) Object.keys(attrs).forEach(function(k) {
      if (k === 'text') node.textContent = attrs[k];
      else if (k === 'cls') node.className = attrs[k];
      else node.setAttribute(k, attrs[k]);
    });
    if (children) children.forEach(function(c) {
      if (typeof c === 'string') node.appendChild(document.createTextNode(c));
      else if (c) node.appendChild(c);
    });
    return node;
  }

  function renderDashboard(data) {
    var app = document.getElementById('app');
    app.textContent = '';

    if (!data || !data.sessions || data.sessions.length === 0) {
      app.appendChild(renderEmptyState());
      return;
    }

    var sessions = data.sessions;
    var totalSessions = sessions.length;
    var totalTokensIn = sessions.reduce(function(s, d) { return s + (d.tokens_in || 0); }, 0);
    var totalTokensOut = sessions.reduce(function(s, d) { return s + (d.tokens_out || 0); }, 0);
    var totalCost = sessions.reduce(function(s, d) { return s + (d.cost || 0); }, 0);
    var totalDuration = sessions.reduce(function(s, d) { return s + (d.duration_s || 0); }, 0);

    var editors = {};
    sessions.forEach(function(s) { var e = s.editor || 'unknown'; editors[e] = (editors[e] || 0) + 1; });
    var models = {};
    sessions.forEach(function(s) { var m = s.model || 'unknown'; models[m] = (models[m] || 0) + 1; });

    var dailyCounts = {};
    var now = new Date();
    for (var i = 29; i >= 0; i--) {
      var d = new Date(now); d.setDate(d.getDate() - i);
      dailyCounts[d.toISOString().slice(0, 10)] = 0;
    }
    sessions.forEach(function(s) {
      var day = (s.timestamp || '').slice(0, 10);
      if (dailyCounts.hasOwnProperty(day)) dailyCounts[day]++;
    });

    app.appendChild(el('h1', { text: 'Analytics Dashboard' }));
    app.appendChild(el('div', { cls: 'subtitle', text: 'Agentlytics \u2014 AI Usage Metrics' }));

    // Metrics grid
    var grid = el('div', { cls: 'metrics-grid' });
    grid.appendChild(metricCard(totalSessions.toString(), 'Sessions'));
    grid.appendChild(metricCard(formatNumber(totalTokensIn), 'Tokens In'));
    grid.appendChild(metricCard(formatNumber(totalTokensOut), 'Tokens Out'));
    grid.appendChild(metricCard('$' + totalCost.toFixed(2), 'Total Cost'));
    grid.appendChild(metricCard(formatDuration(totalDuration), 'Total Time'));
    grid.appendChild(metricCard(Math.round((totalTokensIn / Math.max(totalTokensIn + totalTokensOut, 1)) * 100) + '%', 'Input Ratio'));
    app.appendChild(grid);

    // Activity heatmap
    app.appendChild(section('Activity (Last 30 Days)', renderHeatmap(dailyCounts)));

    // Bar chart
    app.appendChild(section('Daily Sessions', renderBarChart(dailyCounts)));

    // Editor donut
    app.appendChild(section('Editor Breakdown', renderDonut(editors)));

    // Model table
    app.appendChild(section('Model Usage', renderModelTable(models, totalSessions)));
  }

  function section(title, content) {
    var s = el('div', { cls: 'section' });
    s.appendChild(el('div', { cls: 'section-title', text: title }));
    s.appendChild(content);
    return s;
  }

  function metricCard(value, label) {
    var card = el('div', { cls: 'metric-card' });
    card.appendChild(el('div', { cls: 'metric-value', text: value }));
    card.appendChild(el('div', { cls: 'metric-label', text: label }));
    return card;
  }

  function formatNumber(n) {
    if (n >= 1000000) return (n / 1000000).toFixed(1) + 'M';
    if (n >= 1000) return (n / 1000).toFixed(1) + 'K';
    return n.toString();
  }

  function formatDuration(seconds) {
    if (seconds >= 3600) return (seconds / 3600).toFixed(1) + 'h';
    if (seconds >= 60) return Math.round(seconds / 60) + 'm';
    return seconds + 's';
  }

  function renderHeatmap(dailyCounts) {
    var days = Object.keys(dailyCounts).sort();
    var maxVal = Math.max.apply(null, days.map(function(d) { return dailyCounts[d]; }));
    if (maxVal === 0) maxVal = 1;

    var container = el('div', { cls: 'heatmap' });
    days.forEach(function(day) {
      var count = dailyCounts[day];
      var level = count === 0 ? 0 : Math.min(4, Math.ceil((count / maxVal) * 4));
      var cell = el('div', { cls: 'heat-cell heat-' + level, title: day + ': ' + count + ' sessions' });
      container.appendChild(cell);
    });
    return container;
  }

  function renderBarChart(dailyCounts) {
    var days = Object.keys(dailyCounts).sort().slice(-14);
    var maxVal = Math.max.apply(null, days.map(function(d) { return dailyCounts[d]; }));
    if (maxVal === 0) maxVal = 1;

    var chart = el('div', { cls: 'bar-chart' });
    days.forEach(function(day) {
      var count = dailyCounts[day];
      var pct = Math.max(2, (count / maxVal) * 100);
      var col = el('div', { cls: 'bar-col' });
      col.appendChild(el('div', { cls: 'bar', style: 'height:' + pct + '%' }));
      col.appendChild(el('div', { cls: 'bar-label', text: day.slice(8) }));
      chart.appendChild(col);
    });
    return chart;
  }

  var COLORS = ['#89b4fa', '#a6e3a1', '#f9e2af', '#f38ba8', '#cba6f7', '#fab387', '#94e2d5'];

  function renderDonut(data) {
    var entries = Object.keys(data).map(function(k) { return { name: k, count: data[k] }; });
    entries.sort(function(a, b) { return b.count - a.count; });
    var total = entries.reduce(function(s, e) { return s + e.count; }, 0);
    if (total === 0) return el('div', { text: 'No data', style: 'color:var(--zed-text-muted)' });

    var container = el('div', { cls: 'donut-container' });

    // SVG donut using DOM API
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('class', 'donut-svg');
    svg.setAttribute('viewBox', '0 0 80 80');
    var offset = 0;
    entries.forEach(function(entry, i) {
      var pct = (entry.count / total) * 100;
      var circle = document.createElementNS(ns, 'circle');
      circle.setAttribute('cx', '40');
      circle.setAttribute('cy', '40');
      circle.setAttribute('r', '30');
      circle.setAttribute('fill', 'none');
      circle.setAttribute('stroke', COLORS[i % COLORS.length]);
      circle.setAttribute('stroke-width', '12');
      circle.setAttribute('stroke-dasharray', (pct * 1.884) + ' ' + (188.4 - pct * 1.884));
      circle.setAttribute('stroke-dashoffset', '' + (-offset * 1.884));
      svg.appendChild(circle);
      offset += pct;
    });
    container.appendChild(svg);

    var legend = el('div', { cls: 'donut-legend' });
    entries.slice(0, 5).forEach(function(entry, i) {
      var pct = Math.round((entry.count / total) * 100);
      var item = el('div', { cls: 'legend-item' });
      item.appendChild(el('span', { cls: 'legend-dot', style: 'background:' + COLORS[i % COLORS.length] }));
      item.appendChild(document.createTextNode(entry.name + ' (' + pct + '%)'));
      legend.appendChild(item);
    });
    container.appendChild(legend);
    return container;
  }

  function renderModelTable(models, total) {
    var entries = Object.keys(models).map(function(k) { return { name: k, count: models[k] }; });
    entries.sort(function(a, b) { return b.count - a.count; });

    var table = el('table', { cls: 'data-table' });
    var thead = el('thead');
    var headRow = el('tr');
    headRow.appendChild(el('th', { text: 'Model' }));
    headRow.appendChild(el('th', { text: 'Sessions' }));
    headRow.appendChild(el('th', { text: 'Share' }));
    thead.appendChild(headRow);
    table.appendChild(thead);

    var tbody = el('tbody');
    entries.slice(0, 8).forEach(function(entry) {
      var pct = Math.round((entry.count / total) * 100);
      var row = el('tr');
      row.appendChild(el('td', { text: entry.name }));
      row.appendChild(el('td', { text: entry.count.toString() }));
      row.appendChild(el('td', { text: pct + '%' }));
      tbody.appendChild(row);
    });
    table.appendChild(tbody);
    return table;
  }

  function renderEmptyState() {
    var state = el('div', { cls: 'empty-state' });
    state.appendChild(el('h2', { text: 'No Analytics Data' }));
    state.appendChild(el('p', { text: 'Create session logs at ~/.agentics/sessions/ in JSONL format to see your AI usage metrics.' }));
    var hint = el('p', { style: 'margin-top:12px' });
    hint.appendChild(document.createTextNode('Using sample data for demo. '));
    hint.appendChild(el('code', { text: 'analytics.getSessionData' }));
    hint.appendChild(document.createTextNode(' IPC handler provides real data.'));
    state.appendChild(hint);
    return state;
  }

  function generateSampleData() {
    var sessions = [];
    var editors = ['cursor', 'vscode', 'claude-code', 'zed'];
    var models = ['claude-opus-4', 'claude-sonnet-4', 'gpt-4o', 'claude-haiku-3.5', 'gemini-2.5-pro'];
    var now = new Date();

    for (var i = 0; i < 120; i++) {
      var daysAgo = Math.floor(Math.random() * 30);
      var d = new Date(now);
      d.setDate(d.getDate() - daysAgo);
      d.setHours(Math.floor(Math.random() * 14) + 8);
      d.setMinutes(Math.floor(Math.random() * 60));

      sessions.push({
        editor: editors[Math.floor(Math.random() * editors.length)],
        model: models[Math.floor(Math.random() * models.length)],
        tokens_in: Math.floor(Math.random() * 5000) + 500,
        tokens_out: Math.floor(Math.random() * 8000) + 1000,
        cost: Math.random() * 0.5,
        timestamp: d.toISOString(),
        duration_s: Math.floor(Math.random() * 600) + 30
      });
    }
    return { sessions: sessions };
  }

  // Bootstrap
  waitForBridge(function() {
    window.__zed_ipc.invoke('analytics.getSessionData', {})
      .then(function(data) { renderDashboard(data); })
      .catch(function() { renderDashboard(generateSampleData()); });
  });
})();
</script>
</body>
</html>"##;
