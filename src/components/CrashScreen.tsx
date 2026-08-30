import { Component, type ErrorInfo, type ReactNode } from "react";

/** Last line of defence.
 *
 *  When a render throws, React 19 unmounts the whole tree. In a webview that
 *  leaves the page background and nothing else — a silent void with no hint of
 *  what happened and nothing to copy. Anything is better than that. */
export class CrashScreen extends Component<
  { children: ReactNode },
  { error: Error | null; stack: string }
> {
  state = { error: null as Error | null, stack: "" };

  static getDerivedStateFromError(error: Error) {
    return { error, stack: "" };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ error, stack: info.componentStack ?? "" });
  }

  render() {
    const { error, stack } = this.state;
    if (!error) return this.props.children;

    const report = `${error.name}: ${error.message}\n${error.stack ?? ""}\n${stack}`;
    // Deliberately plain DOM and inline styles: the theme provider is part of
    // the tree that just died, so nothing here may depend on it.
    return (
      <div
        style={{
          height: "100%",
          overflow: "auto",
          padding: 24,
          font: "14px system-ui, sans-serif",
          color: "#DFE3E7",
          background: "#0F1417",
        }}
      >
        <h2 style={{ margin: "0 0 8px" }}>TurboGrab s'est arrêté</h2>
        <p style={{ margin: "0 0 16px", color: "#BFC8CE" }}>
          Une erreur d'affichage a interrompu l'application. Tes téléchargements
          sont intacts — ils vivent côté Rust, pas dans cette fenêtre.
        </p>
        <div style={{ display: "flex", gap: 8, marginBottom: 16 }}>
          <button
            onClick={() => location.reload()}
            style={{
              font: "inherit",
              padding: "8px 16px",
              borderRadius: 999,
              border: 0,
              background: "#86CFFF",
              color: "#003548",
              cursor: "pointer",
            }}
          >
            Recharger
          </button>
          <button
            onClick={() => navigator.clipboard?.writeText(report)}
            style={{
              font: "inherit",
              padding: "8px 16px",
              borderRadius: 999,
              border: "1px solid #40484D",
              background: "transparent",
              color: "inherit",
              cursor: "pointer",
            }}
          >
            Copier le détail
          </button>
        </div>
        <pre
          style={{
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            fontSize: 12,
            padding: 12,
            borderRadius: 8,
            background: "#0A0F12",
            border: "1px solid #40484D",
            userSelect: "text",
          }}
        >
          {report}
        </pre>
      </div>
    );
  }
}
