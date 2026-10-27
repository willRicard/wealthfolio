import i18n from "i18next";
import {
  deletePasskey,
  listPasskeys,
  PasskeyCancelled,
  registerPasskey,
  renamePasskey,
} from "@/features/passkeys/api";
import { QueryKeys } from "@/lib/query-keys";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wealthfolio/ui/components/ui/use-toast";

function reportFailure(titleKey: string, error: unknown) {
  if (error instanceof PasskeyCancelled) return;
  toast({
    title: i18n.t(titleKey),
    description: error instanceof Error ? error.message : String(error),
    variant: "destructive",
  });
}

export function usePasskeys() {
  const queryClient = useQueryClient();
  const invalidate = () => queryClient.invalidateQueries({ queryKey: [QueryKeys.PASSKEYS] });

  const passkeysQuery = useQuery({
    queryKey: [QueryKeys.PASSKEYS],
    queryFn: listPasskeys,
  });

  const createMutation = useMutation({
    mutationFn: (name: string) => registerPasskey(name),
    onSuccess: async () => {
      await invalidate();
      toast({ title: i18n.t("settings:passkeys.add_success"), variant: "success" });
    },
    onError: (error) => reportFailure("settings:passkeys.add_failed", error),
  });

  const renameMutation = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => renamePasskey(id, name),
    onSuccess: invalidate,
    onError: (error) => reportFailure("settings:passkeys.rename_failed", error),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deletePasskey(id),
    onSuccess: async () => {
      await invalidate();
      toast({ title: i18n.t("settings:passkeys.remove_success"), variant: "success" });
    },
    onError: (error) => reportFailure("settings:passkeys.remove_failed", error),
  });

  return {
    configured: passkeysQuery.data?.configured ?? false,
    passkeys: passkeysQuery.data?.passkeys ?? [],
    isLoading: passkeysQuery.isLoading,
    createMutation,
    renameMutation,
    deleteMutation,
  };
}
