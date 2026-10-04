import "./App.css";
import { AppProvider, useApp } from "./state/AppContext";
import { HomeScreen } from "./routes/HomeScreen";
import { ImportScreen } from "./routes/ImportScreen";
import { LibraryScreen } from "./routes/LibraryScreen";
import { ActivityScreen } from "./routes/ActivityScreen";
import { SitesScreen } from "./routes/SitesScreen";
import { ConnectionsScreen } from "./routes/ConnectionsScreen";
import { SettingsScreen } from "./routes/SettingsScreen";
import { AboutScreen } from "./routes/AboutScreen";
import { ToastStack } from "./components/Toast";

function Router() {
  const app = useApp();
  switch (app.screen) {
    case "home":
      return <HomeScreen />;
    case "import":
      return <ImportScreen />;
    case "library":
      return <LibraryScreen />;
    case "activity":
      return <ActivityScreen />;
    case "sites":
      return <SitesScreen />;
    case "connections":
      return <ConnectionsScreen />;
    case "settings":
      return <SettingsScreen />;
    case "about":
      return <AboutScreen />;
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
