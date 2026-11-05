import { useTranslation } from "react-i18next";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Icons,
  Label,
  Separator,
  Switch,
} from "@wealthfolio/ui";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from "@wealthfolio/ui/components/ui/item";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";

import { usePushNotifications } from "@/features/push-notifications/use-push-notifications";
import { SettingsHeader } from "../settings-header";

export default function NotificationsPage() {
  const { t } = useTranslation();
  const {
    supported,
    needsHomeScreenInstall,
    permission,
    serverEnabled,
    loading,
    subscribed,
    preferences,
    enable,
    disable,
    sendTest,
    savePreferences,
    setPreference,
  } = usePushNotifications();

  const handleEnable = () => {
    enable.mutate(undefined, {
      onError: (error: Error) => {
        toast({
          title: t("settings:notifications.enable_failed_title"),
          description: error.message,
          variant: "destructive",
        });
      },
    });
  };

  const handleDisable = () => {
    disable.mutate(undefined, {
      onError: (error: Error) => {
        toast({
          title: t("settings:notifications.disable_failed_title"),
          description: error.message,
          variant: "destructive",
        });
      },
    });
  };

  const handleTest = () => {
    sendTest.mutate(undefined, {
      onSuccess: ({ delivered }) => {
        toast(
          delivered === 0
            ? {
                title: t("settings:notifications.test_none_title"),
                description: t("settings:notifications.test_none_description"),
                variant: "destructive",
              }
            : {
                title: t("settings:notifications.test_sent_title"),
                description: t("settings:notifications.test_sent_description"),
              },
        );
      },
      onError: (error: Error) => {
        toast({
          title: t("settings:notifications.test_failed_title"),
          description: error.message,
          variant: "destructive",
        });
      },
    });
  };

  return (
    <div className="space-y-6">
      <SettingsHeader
        heading={t("settings:notifications.page_heading")}
        text={t("settings:notifications.page_text")}
      />
      <Separator />

      {!supported && (
        <Alert>
          <Icons.AlertTriangle className="size-4" />
          <AlertTitle>{t("settings:notifications.unsupported_title")}</AlertTitle>
          <AlertDescription>{t("settings:notifications.unsupported_description")}</AlertDescription>
        </Alert>
      )}

      {supported && needsHomeScreenInstall && (
        <Alert>
          <Icons.Info className="size-4" />
          <AlertTitle>{t("settings:notifications.ios_install_title")}</AlertTitle>
          <AlertDescription>{t("settings:notifications.ios_install_description")}</AlertDescription>
        </Alert>
      )}

      {supported && !loading && !serverEnabled && (
        <Alert variant="destructive">
          <Icons.AlertTriangle className="size-4" />
          <AlertTitle>{t("settings:notifications.server_disabled_title")}</AlertTitle>
          <AlertDescription>
            {t("settings:notifications.server_disabled_description")}
          </AlertDescription>
        </Alert>
      )}

      {supported && permission === "denied" && (
        <Alert variant="destructive">
          <Icons.AlertTriangle className="size-4" />
          <AlertTitle>{t("settings:notifications.denied_title")}</AlertTitle>
          <AlertDescription>{t("settings:notifications.denied_description")}</AlertDescription>
        </Alert>
      )}

      {supported && serverEnabled && (
        <>
          <Card>
            <CardHeader>
              <CardTitle className="text-lg">
                {t("settings:notifications.this_device_title")}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <Item className="p-0">
                <ItemContent>
                  <ItemTitle>
                    <Label>{t("settings:notifications.this_device_label")}</Label>
                  </ItemTitle>
                  <ItemDescription className="line-clamp-none">
                    {subscribed
                      ? t("settings:notifications.this_device_on")
                      : t("settings:notifications.this_device_off")}
                  </ItemDescription>
                </ItemContent>
                <ItemActions>
                  {subscribed ? (
                    <Button
                      variant="outline"
                      onClick={handleDisable}
                      disabled={disable.isPending || loading}
                    >
                      {t("settings:notifications.disable_action")}
                    </Button>
                  ) : (
                    <Button
                      onClick={handleEnable}
                      disabled={enable.isPending || loading || permission === "denied"}
                    >
                      {t("settings:notifications.enable_action")}
                    </Button>
                  )}
                </ItemActions>
              </Item>

              {subscribed && (
                <Item className="p-0">
                  <ItemContent>
                    <ItemTitle>
                      <Label>{t("settings:notifications.test_label")}</Label>
                    </ItemTitle>
                    <ItemDescription className="line-clamp-none">
                      {t("settings:notifications.test_description")}
                    </ItemDescription>
                  </ItemContent>
                  <ItemActions>
                    <Button variant="outline" onClick={handleTest} disabled={sendTest.isPending}>
                      {t("settings:notifications.test_action")}
                    </Button>
                  </ItemActions>
                </Item>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-lg">{t("settings:notifications.kinds_title")}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <ItemDescription className="line-clamp-none">
                {t("settings:notifications.kinds_description")}
              </ItemDescription>

              <Item className="p-0">
                <ItemContent>
                  <ItemTitle>
                    <Label htmlFor="push-transactions">
                      {t("settings:notifications.transactions_label")}
                    </Label>
                  </ItemTitle>
                  <ItemDescription className="line-clamp-none">
                    {t("settings:notifications.transactions_description")}
                  </ItemDescription>
                </ItemContent>
                <ItemActions>
                  <Switch
                    id="push-transactions"
                    checked={preferences?.transactions ?? false}
                    onCheckedChange={(value) => setPreference("transactions", value)}
                    disabled={!preferences || savePreferences.isPending}
                  />
                </ItemActions>
              </Item>

              <Item className="p-0">
                <ItemContent>
                  <ItemTitle>
                    <Label htmlFor="push-budgets">
                      {t("settings:notifications.budgets_label")}
                    </Label>
                  </ItemTitle>
                  <ItemDescription className="line-clamp-none">
                    {t("settings:notifications.budgets_description")}
                  </ItemDescription>
                </ItemContent>
                <ItemActions>
                  <Switch
                    id="push-budgets"
                    checked={preferences?.budgets ?? false}
                    onCheckedChange={(value) => setPreference("budgets", value)}
                    disabled={!preferences || savePreferences.isPending}
                  />
                </ItemActions>
              </Item>
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
