import { useEffect, useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useTranslation } from "react-i18next";
import {
  createProcurementItem,
  updateProcurementItem,
  uploadProcurementPhotoDraft,
  discardProcurementPhotoDraft,
} from "@/adapters";
import { useTaxonomy } from "@/hooks/use-taxonomies";
import { Button, Input, StarRating, Icons, ResponsiveSelect } from "@wealthfolio/ui";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@wealthfolio/ui/components/ui/dialog";
import {
  Form,
  FormField,
  FormItem,
  FormLabel,
  FormControl,
  FormMessage,
} from "@wealthfolio/ui/components/ui/form";
import { Textarea } from "@wealthfolio/ui/components/ui/textarea";
import { AdvancedOptionsSection } from "@/pages/activity/components/forms/fields/advanced-options-section";
import { QuickCategorizePopover } from "@/features/spending/components/quick-categorize-popover";
import { ItemPhoto } from "./item-photo";
import type { ProcurementItem } from "../types";
import { DEFAULT_PROCUREMENT_UNIT, PROCUREMENT_UNITS } from "../units";
import {
  DEFAULT_PROCUREMENT_TAX_TYPE,
  PROCUREMENT_TAX_TYPES,
  PROCUREMENT_TAX_TYPE_LABELS,
} from "../tax-types";

const unitOptions = PROCUREMENT_UNITS.map((unit) => ({ value: unit, label: unit }));

interface ItemFormProps {
  item?: ProcurementItem;
  onClose: () => void;
  onSaved: () => void;
}
export function ItemForm({ item, onClose, onSaved }: ItemFormProps) {
  const { t } = useTranslation("procurement");
  const schema = z.object({
    name: z.string().trim().min(1, t("required")),
    brand: z.string(),
    memo: z.string(),
    categoryId: z.string().min(1, t("required")),
    subcategoryId: z.string(),
    rating: z.number().min(0).max(5).int(t("invalidRating")).nullable(),
    unit: z
      .string()
      .refine(
        (value) => (PROCUREMENT_UNITS as readonly string[]).includes(value),
        t("invalidUnit"),
      ),
    taxType: z
      .string()
      .refine(
        (value) => (PROCUREMENT_TAX_TYPES as readonly string[]).includes(value),
        t("invalidTaxType"),
      ),
    aliases: z.array(z.string()),
  });
  const form = useForm<z.infer<typeof schema>>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: item?.name ?? "",
      brand: item?.brand ?? "",
      memo: item?.memo ?? "",
      categoryId: item?.categoryId ?? "",
      subcategoryId: item?.subcategoryId ?? "",
      rating: item?.rating ?? null,
      unit: item?.unit ?? DEFAULT_PROCUREMENT_UNIT,
      taxType: item?.taxType ?? DEFAULT_PROCUREMENT_TAX_TYPE,
      aliases: item?.aliases ?? [],
    },
  });
  const { data: taxonomy } = useTaxonomy("procurement_categories");
  const [editingAlias, setEditingAlias] = useState<number | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const category = form.watch("categoryId");
  const subcategory = form.watch("subcategoryId");
  const categories = taxonomy?.categories ?? [];
  const selectedCategory = categories.find((c) => c.id === (subcategory || category));
  const parentCategory = categories.find((c) => c.id === selectedCategory?.parentId);
  const draftId = useRef<string | undefined>(undefined);
  const mounted = useRef(true);
  const [preview, setPreview] = useState<string>();
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string>();
  const [photoError, setPhotoError] = useState<string>();
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (draftId.current)
        void discardProcurementPhotoDraft(draftId.current).catch(() => undefined);
    };
  }, []);
  useEffect(
    () => () => {
      if (preview) URL.revokeObjectURL(preview);
    },
    [preview],
  );
  async function upload(file: File) {
    if (uploading || form.formState.isSubmitting) return;
    setUploading(true);
    setPhotoError(undefined);
    try {
      if (draftId.current) {
        await discardProcurementPhotoDraft(draftId.current);
        draftId.current = undefined;
        if (mounted.current) setPreview(undefined);
      }
      const draft = await uploadProcurementPhotoDraft(file);
      if (!mounted.current) {
        await discardProcurementPhotoDraft(draft.id);
        return;
      }
      draftId.current = draft.id;
      setPreview(URL.createObjectURL(file));
    } catch {
      if (mounted.current) setPhotoError(t("uploadFailed"));
    } finally {
      if (mounted.current) setUploading(false);
    }
  }
  async function close() {
    if (form.formState.isSubmitting) return;
    try {
      const discardedId = draftId.current;
      if (discardedId) {
        await discardProcurementPhotoDraft(discardedId);
        if (draftId.current === discardedId) draftId.current = undefined;
      }
      onClose();
    } catch {
      setError(t("error"));
    }
  }
  const submit = form.handleSubmit(async (values) => {
    setError(undefined);
    try {
      const input = {
        ...values,
        brand: values.brand || null,
        memo: values.memo || null,
        subcategoryId: values.subcategoryId || null,
        aliases: values.aliases.map((alias) => alias.trim()).filter(Boolean),
        draftId: draftId.current,
      };
      if (item) await updateProcurementItem(item.id, input);
      else await createProcurementItem(input);
      draftId.current = undefined;
      onSaved();
    } catch {
      setError(t("error"));
    }
  });
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) void close();
      }}
    >
      <DialogContent
        showCloseButton={false}
        className="flex max-h-[90dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg"
        mobileClassName="h-[90dvh]"
      >
        <DialogHeader className="relative shrink-0 border-b p-6 pr-14 text-left">
          <DialogTitle>{t(item ? "edit" : "add")}</DialogTitle>
          <DialogDescription>{t("subtitle")}</DialogDescription>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="absolute right-3 top-3"
            aria-label={t("close")}
            onClick={() => void close()}
          >
            <Icons.X className="size-4" />
          </Button>
        </DialogHeader>
        <Form {...form}>
          <form onSubmit={submit} className="flex min-h-0 flex-1 flex-col">
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-6">
              <FormField
                control={form.control}
                name="name"
                render={({ field }) => (
                  <FormItem className="grid min-w-0 grid-cols-[4rem_minmax(0,1fr)] items-center gap-x-3 gap-y-2 space-y-0">
                    <FormLabel className="col-start-2">{t("name")}</FormLabel>
                    <div className="col-start-1 row-start-2">
                      <button
                        type="button"
                        aria-label={t("photo")}
                        disabled={uploading || form.formState.isSubmitting}
                        className="bg-muted relative flex size-16 items-center justify-center overflow-hidden rounded-full border"
                        onClick={() => fileInput.current?.click()}
                        onDragOver={(event) => event.preventDefault()}
                        onDrop={(event) => {
                          event.preventDefault();
                          const file = event.dataTransfer.files[0];
                          if (file) void upload(file);
                        }}
                      >
                        {preview ? (
                          <img
                            src={preview}
                            alt={form.watch("name")}
                            className="size-full object-cover"
                          />
                        ) : item?.photo ? (
                          <ItemPhoto item={item} className="size-full rounded-full object-cover" />
                        ) : (
                          <Icons.ImageUp className="size-6" />
                        )}
                        {uploading && (
                          <span
                            role="status"
                            aria-label={t("uploading")}
                            className="bg-background/70 absolute inset-0 flex items-center justify-center"
                          >
                            <Icons.Spinner className="size-6 animate-spin" />
                          </span>
                        )}
                      </button>
                      <input
                        ref={fileInput}
                        id="procurement-photo"
                        aria-label={t("photo")}
                        className="hidden"
                        type="file"
                        accept="image/jpeg,image/png,image/webp"
                        disabled={uploading || form.formState.isSubmitting}
                        onChange={(event) => {
                          const file = event.target.files?.[0];
                          event.target.value = "";
                          if (file) void upload(file);
                        }}
                      />
                    </div>
                    <FormControl className="col-start-2 row-start-2">
                      <Input {...field} autoFocus />
                    </FormControl>
                    <FormMessage className="col-start-2" />
                  </FormItem>
                )}
              />
              {photoError && (
                <p role="alert" className="text-destructive text-sm">
                  {photoError}
                </p>
              )}
              <div className="grid grid-cols-[minmax(0,1fr)_5rem] items-start gap-3">
                <FormField
                  control={form.control}
                  name="brand"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{t("brand")}</FormLabel>
                      <FormControl>
                        <Input {...field} />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
                <FormField
                  control={form.control}
                  name="unit"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{t("unit")}</FormLabel>
                      <FormControl>
                        <ResponsiveSelect
                          value={field.value}
                          onValueChange={field.onChange}
                          options={unitOptions}
                          placeholder={t("unit")}
                          sheetTitle={t("unitSheetTitle")}
                        />
                      </FormControl>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </div>
              <FormField
                control={form.control}
                name="taxType"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("taxType")}</FormLabel>
                    <FormControl>
                      <ResponsiveSelect
                        value={field.value}
                        onValueChange={field.onChange}
                        options={PROCUREMENT_TAX_TYPES.map((value) => ({
                          value,
                          label: t(PROCUREMENT_TAX_TYPE_LABELS[value]),
                        }))}
                        placeholder={t("taxType")}
                        sheetTitle={t("taxTypeSheetTitle")}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="categoryId"
                render={() => (
                  <FormItem>
                    <FormLabel>{t("category")}</FormLabel>
                    <QuickCategorizePopover
                      taxonomyId="procurement_categories"
                      selectedCategoryId={selectedCategory?.id}
                      onSelect={(_, id) => {
                        const selected = categories.find((c) => c.id === id);
                        form.setValue("categoryId", selected?.parentId ?? id, {
                          shouldDirty: true,
                          shouldValidate: true,
                        });
                        form.setValue("subcategoryId", selected?.parentId ? id : "", {
                          shouldDirty: true,
                        });
                      }}
                      trigger={
                        <FormControl>
                          <Button
                            type="button"
                            variant="outline"
                            className="w-full justify-between"
                          >
                            <span className="flex min-w-0 items-center gap-2">
                              {selectedCategory && (
                                <span
                                  className="size-2.5 shrink-0 rounded-full"
                                  style={{
                                    backgroundColor:
                                      selectedCategory.color ?? "var(--muted-foreground)",
                                  }}
                                />
                              )}
                              <span className="truncate">
                                {selectedCategory
                                  ? [parentCategory?.name, selectedCategory.name]
                                      .filter(Boolean)
                                      .join(" / ")
                                  : t("category")}
                              </span>
                            </span>
                            <Icons.ChevronDown className="size-4 shrink-0" />
                          </Button>
                        </FormControl>
                      }
                    />
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="rating"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("rating")}</FormLabel>
                    <div className="flex items-center gap-3">
                      <StarRating
                        rating={field.value ?? 0}
                        interactive
                        size="lg"
                        onRatingChange={field.onChange}
                      />
                      <span className="text-muted-foreground text-sm">{field.value ?? "—"}</span>
                      {field.value !== null && (
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          aria-label={t("clearRating")}
                          onClick={() => field.onChange(null)}
                        >
                          <Icons.X className="size-4" />
                        </Button>
                      )}
                    </div>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <FormField
                control={form.control}
                name="memo"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("memo")}</FormLabel>
                    <FormControl>
                      <Textarea {...field} />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />
              <AdvancedOptionsSection
                title={t("advanced")}
                dashed
                showCurrency={false}
                showFxRate={false}
                showSubtype={false}
              >
                <FormField
                  control={form.control}
                  name="aliases"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel>{t("aliases")}</FormLabel>
                      <table className="w-full text-sm">
                        <tbody>
                          {field.value.map((alias, index) => (
                            <tr key={index} className="border-b">
                              <td className="w-full py-2 pr-2">
                                {editingAlias === index ? (
                                  <Input
                                    aria-label={`${t("aliases")} ${index + 1}`}
                                    autoFocus
                                    value={alias}
                                    onChange={(event) =>
                                      field.onChange(
                                        field.value.map((value, i) =>
                                          i === index ? event.target.value : value,
                                        ),
                                      )
                                    }
                                    onBlur={() => setEditingAlias(null)}
                                    onKeyDown={(event) => {
                                      if (event.key === "Enter") {
                                        event.preventDefault();
                                        setEditingAlias(null);
                                      }
                                    }}
                                  />
                                ) : (
                                  <button
                                    type="button"
                                    className="w-full text-left"
                                    onClick={() => setEditingAlias(index)}
                                  >
                                    <p className="min-h-5 whitespace-pre-wrap break-words">
                                      {alias || t("aliasesHint")}
                                    </p>
                                  </button>
                                )}
                              </td>
                              <td>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="icon"
                                  aria-label={`${t("delete")} ${t("aliases")} ${index + 1}`}
                                  onClick={() => {
                                    field.onChange(field.value.filter((_, i) => i !== index));
                                    setEditingAlias(null);
                                  }}
                                >
                                  <Icons.Trash className="size-4" />
                                </Button>
                              </td>
                            </tr>
                          ))}
                          <tr>
                            <td colSpan={2} className="pt-2">
                              <Button
                                type="button"
                                variant="ghost"
                                className="text-muted-foreground w-full justify-start border border-dashed"
                                onClick={() => {
                                  setEditingAlias(field.value.length);
                                  field.onChange([...field.value, ""]);
                                }}
                              >
                                <Icons.Plus className="mr-2 size-4" />
                                {t("addAlias")}
                              </Button>
                            </td>
                          </tr>
                        </tbody>
                      </table>
                      <FormMessage />
                    </FormItem>
                  )}
                />
              </AdvancedOptionsSection>
              {error && (
                <p role="alert" className="text-destructive text-sm">
                  {error}
                </p>
              )}
            </div>
            <DialogFooter className="shrink-0 border-t p-4">
              <Button type="submit" disabled={uploading || form.formState.isSubmitting}>
                {t("save")}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
