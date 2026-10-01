import i18n from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";
import en from "@/locales/en.json";
import zh from "@/locales/zh.json";

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en }, zh: { translation: zh } },
    supportedLngs: ["en", "zh"],
    fallbackLng: "en",
    load: "languageOnly",
    initAsync: false,
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "codex-speed-language",
      caches: ["localStorage"],
    },
    interpolation: { escapeValue: false },
  });

export function locale(): string {
  return i18n.resolvedLanguage === "zh" ? "zh-CN" : "en-US";
}
function updateDocument() {
  document.documentElement.lang = locale();
  document.title = i18n.t("page_title");
  document
    .querySelector('meta[name="description"]')
    ?.setAttribute("content", i18n.t("page_description"));
}
i18n.on("languageChanged", updateDocument);
updateDocument();
export default i18n;
