import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useTheme } from "./hooks/useTheme";
import { useSettings } from "./hooks/useSettings";
import SettingsWindow from "./components/Settings/SettingsWindow";

function App() {
  const { settings } = useSettings();
  const { i18n } = useTranslation();
  useTheme(settings.theme);

  // Sync i18n language with persisted settings
  useEffect(() => {
    if (settings.ui_language && settings.ui_language !== i18n.language) {
      i18n.changeLanguage(settings.ui_language);
    }
  }, [settings.ui_language, i18n]);

  return <SettingsWindow />;
}

export default App;
