import "./App.css";
import { AppProvider, useApp } from "./state/AppContext";
import { ImportScreen } from "./routes/ImportScreen";
import { LibraryScreen } from "./routes/LibraryScreen";
import { AssetEditorScreen } from "./routes/AssetEditorScreen";
import { ReviewScreen } from "./routes/ReviewScreen";
import { QueueScreen } from "./routes/QueueScreen";
import { SitesScreen } from "./routes/SitesScreen";
import { SettingsScreen } from "./routes/SettingsScreen";
import { ToastStack } from "./components/Toast";

function Router() {
  const app = useApp();
  switch (app.screen) {
    case "import":
      return <ImportScreen />;
    case "library":
      return <LibraryScreen />;
    case "editor":
      return <AssetEditorScreen />;
    case "review":
      return <ReviewScreen />;
    case "queue":
      return <QueueScreen />;
    case "sites":
      return <SitesScreen />;
    case "settings":
      return <SettingsScreen />;
  }
}

function ToastHost() {
  const app = useApp();
  return <ToastStack toasts={app.toasts} onDismiss={app.dismissToast} />;
}

export default function App() {
  return (
    <AppProvider>
      <Router />
      <ToastHost />
    </AppProvider>
  );
}
