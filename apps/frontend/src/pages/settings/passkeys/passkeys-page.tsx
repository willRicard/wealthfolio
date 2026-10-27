import { useTranslation } from "react-i18next";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Icons,
  Separator,
} from "@wealthfolio/ui";
import { isTauri } from "@/adapters";
import { SettingsHeader } from "../settings-header";
import { PasskeyTable } from "./components/passkey-table";

export default function PasskeysPage() {
  const { t } = useTranslation();

  return (
    <div className="space-y-6">
      <SettingsHeader
        heading={t("settings:passkeys.page_heading")}
        text={t("settings:passkeys.page_text")}
      />
      <Separator />

      {isTauri ? (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Icons.ShieldCheck />
            </EmptyMedia>
            <EmptyTitle>{t("settings:passkeys.desktop_unavailable_title")}</EmptyTitle>
            <EmptyDescription>
              {t("settings:passkeys.desktop_unavailable_description")}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <PasskeyTable />
      )}
    </div>
  );
}
