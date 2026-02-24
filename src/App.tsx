import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTheme } from "./hooks/useTheme";
import { useSettings } from "./hooks/useSettings";
import SettingsWindow from "./components/Settings/SettingsWindow";
import Overlay from "./components/Overlay";

function App() {
  const [windowLabel] = useState(() => getCurrentWindow().label);
  const { settings } = useSettings();
  const { i18n } = useTranslation();
  useTheme(settings.theme);

  // Sync i18n language with persisted settings
  useEffect(() => {
    if (settings.ui_language && settings.ui_language !== i18n.language) {
      i18n.changeLanguage(settings.ui_language);
    }
  }, [settings.ui_language, i18n]);

  if (windowLabel === "overlay") {
    return <Overlay />;
  }

  return <SettingsWindow />;
}

export default App;
