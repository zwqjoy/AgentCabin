<script lang="ts">
  import { onMount } from "svelte";
  import type { UserSettings } from "$lib/types";
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import { dbgWarn } from "$lib/utils/debug";
  import SettingsCard from "../SettingsCard.svelte";
  import SettingsRow from "../SettingsRow.svelte";
  import SettingsToggleRow from "../SettingsToggleRow.svelte";
  import SettingsSection from "../SettingsSection.svelte";
  import PetPreview from "$lib/pet/PetPreview.svelte";
  import GrokBotCustomizer from "$lib/pet/GrokBotCustomizer.svelte";
  import {
    BUILTIN_PET_OPTIONS,
    PET_SCALE_MAX,
    PET_SCALE_MIN,
    petSettingsPatch,
    resolvePetSettings,
    type PetSettings,
  } from "$lib/pet/pet-settings";
  import {
    createCustomPet,
    inspectCustomPetImage,
    inspectCustomPetZip,
    listCustomPets,
    type CustomPetOption,
  } from "$lib/pet/custom-pets";
  import { formatPetError } from "$lib/pet/pet-errors";
  import Modal from "$lib/components/Modal.svelte";
  import Button from "$lib/components/Button.svelte";

  interface Props {
    settings: UserSettings;
    onUpdateSettings: (patch: Partial<UserSettings>) => Promise<void>;
  }

  let { settings, onUpdateSettings }: Props = $props();

  let petSettings = $derived(resolvePetSettings(settings));

  let customPets = $state<CustomPetOption[]>([]);
  let petScaleDraft = $state(1);

  let selectedPet = $derived(
    customPets.find((pet) => pet.id === petSettings.id) ??
      BUILTIN_PET_OPTIONS.find((pet) => pet.id === petSettings.id) ??
      BUILTIN_PET_OPTIONS[0],
  );

  // Custom pet modal state
  let petAddOpen = $state(false);
  let petCreateBusy = $state(false);
  let petCreateError = $state("");
  let petAddMode = $state<"image" | "zip">("image");
  let petImageBase64 = $state("");
  let petImageName = $state("");
  let petZipBase64 = $state("");
  let petZipName = $state("");
  let petForm = $state({ slug: "", displayName: "", description: "" });

  async function savePet(patch: Partial<PetSettings>) {
    await onUpdateSettings(petSettingsPatch({ ...petSettings, ...patch }));
  }

  async function refreshCustomPets() {
    try {
      customPets = await listCustomPets();
    } catch (error) {
      dbgWarn("settings", "load custom pets failed", error);
      customPets = [];
    }
  }

  function resetPetCreateForm() {
    petAddOpen = false;
    petCreateBusy = false;
    petCreateError = "";
    petAddMode = "image";
    petImageBase64 = "";
    petImageName = "";
    petZipBase64 = "";
    petZipName = "";
    petForm = { slug: "", displayName: "", description: "" };
  }

  function normalizePetSlug(value: string): string {
    return value
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .replace(/-{2,}/g, "-")
      .slice(0, 73);
  }

  async function readSelectedPetFile(
    title: string,
    extensions: string[],
  ): Promise<{ base64: string; name: string } | null> {
    const { open } = await import("$lib/platform/dialog");
    const selected = await open({
      multiple: false,
      title,
      filters: [{ name: "Pet asset", extensions }],
    });
    if (!selected || typeof selected !== "string") return null;

    const separator = Math.max(selected.lastIndexOf("/"), selected.lastIndexOf("\\"));
    const cwd = separator > 0 ? selected.slice(0, separator) : "/";
    const [base64] = await getTransport().invoke<[string, string]>("read_file_base64", {
      path: selected,
      cwd,
    });
    return { base64, name: selected.slice(separator + 1) };
  }

  async function choosePetImage() {
    try {
      const file = await readSelectedPetFile(t("settings_pet_pickImage"), [
        "png",
        "webp",
        "gif",
        "apng",
      ]);
      if (!file) return;
      await inspectCustomPetImage(file.base64, file.name);
      petImageBase64 = file.base64;
      petImageName = file.name;
      petZipBase64 = "";
      petZipName = "";
      petCreateError = "";
    } catch (error) {
      petCreateError = formatPetError(error, t("settings_pet_createFailed"));
    }
  }

  async function choosePetZip() {
    try {
      const file = await readSelectedPetFile(t("settings_pet_pickZip"), ["zip"]);
      if (!file) return;
      await inspectCustomPetZip(file.base64);
      petZipBase64 = file.base64;
      petZipName = file.name;
      petImageBase64 = "";
      petImageName = "";
      petCreateError = "";
    } catch (error) {
      petCreateError = formatPetError(error, t("settings_pet_createFailed"));
    }
  }

  async function submitCustomPet() {
    const slug = normalizePetSlug(petForm.slug);
    const hasSource = petAddMode === "image" ? petImageBase64 : petZipBase64;
    if (
      petCreateBusy ||
      !hasSource ||
      !slug ||
      !petForm.displayName.trim() ||
      !petForm.description.trim()
    ) {
      return;
    }
    petForm.slug = slug;
    petCreateBusy = true;
    petCreateError = "";
    try {
      const created = await createCustomPet({
        slug,
        displayName: petForm.displayName.trim(),
        description: petForm.description.trim(),
        ...(petAddMode === "image"
          ? { imageBase64: petImageBase64, imageName: petImageName }
          : { zipBase64: petZipBase64 }),
      });
      customPets = [...customPets, created];
      await savePet({ id: created.id });
      resetPetCreateForm();
    } catch (error) {
      petCreateError = formatPetError(error, t("settings_pet_createFailed"));
    } finally {
      petCreateBusy = false;
    }
  }

  async function openCustomPetsFolder() {
    try {
      await getTransport().invoke("open_custom_pets_folder");
    } catch (error) {
      dbgWarn("settings", "open custom pets folder failed", error);
    }
  }

  $effect(() => {
    petScaleDraft = petSettings.scale;
  });

  onMount(() => {
    refreshCustomPets();
  });
</script>

<div class="space-y-8">
  <!-- 1. 桌宠开关与基础行为 -->
  <SettingsSection
    title="桌面宠物 (Desktop Pet)"
    description="在桌面上显示陪伴助手，实时响应编码状态与鼠标互动"
  >
    <SettingsCard>
      <SettingsToggleRow
        id="pet-enabled"
        title="启用桌面宠物"
        description="在屏幕角落显示浮动桌面宠物精灵"
        checked={petSettings.enabled}
        onchange={(checked) => savePet({ enabled: checked })}
      />

      <SettingsToggleRow
        id="pet-always-on-top"
        title="总在最前 (Always on Top)"
        description="保持宠物窗口始终漂浮在其他窗口上方"
        checked={petSettings.alwaysOnTop}
        onchange={(checked) => savePet({ alwaysOnTop: checked })}
      />

      <SettingsToggleRow
        id="pet-patrol"
        title="巡逻漫步 (Patrol Mode)"
        description="宠物将在屏幕边缘或底部定时踱步走动"
        checked={petSettings.patrolEnabled}
        onchange={(checked) => savePet({ patrolEnabled: checked })}
      />

      <SettingsToggleRow
        id="pet-edgesnap"
        title="边缘吸附 (Snap to Edge)"
        description="拖拽宠物靠近屏幕边缘时自动吸附固定"
        checked={petSettings.snapToEdge}
        onchange={(checked) => savePet({ snapToEdge: checked })}
      />

      <SettingsToggleRow
        id="pet-click"
        title="点击互动反应"
        description="点击宠物时触发特殊音效与趣味动作反应"
        checked={petSettings.clickInteractionEnabled}
        onchange={(checked) => savePet({ clickInteractionEnabled: checked })}
      />

      <SettingsRow
        title="宠物尺寸缩放"
        description="当前缩放比: {Math.round(petScaleDraft * 100)}%"
      >
        <div class="flex items-center gap-3 w-48">
          <input
            type="range"
            min={PET_SCALE_MIN}
            max={PET_SCALE_MAX}
            step="0.05"
            class="w-full accent-primary"
            value={petScaleDraft}
            oninput={(e) => {
              petScaleDraft = Number((e.target as HTMLInputElement).value);
            }}
            onchange={() => {
              if (petScaleDraft !== petSettings.scale) {
                savePet({ scale: petScaleDraft });
              }
            }}
          />
          <span class="w-12 text-right font-mono text-xs text-muted-foreground"
            >{Math.round(petScaleDraft * 100)}%</span
          >
        </div>
      </SettingsRow>
    </SettingsCard>
  </SettingsSection>

  <!-- 2. 内置宠物选择 -->
  <SettingsSection title="宠物模型选择" description="挑选内置或已安装的桌面宠物形象">
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3.5">
      {#each BUILTIN_PET_OPTIONS as pet (pet.id)}
        {@const isSelected = pet.id === selectedPet.id}
        <div
          class="flex items-center gap-4 rounded-xl border p-4 transition-all cursor-pointer {isSelected
            ? 'border-primary bg-primary/[0.06] shadow-xs ring-1 ring-primary/30'
            : 'border-border/70 bg-card hover:border-primary/40'}"
          onclick={() => savePet({ id: pet.id })}
          role="button"
          tabindex="0"
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              savePet({ id: pet.id });
            }
          }}
        >
          <div
            class="h-16 w-16 shrink-0 overflow-hidden rounded-xl bg-muted/30 flex items-center justify-center pointer-events-none"
          >
            <PetPreview petId={pet.id} size="thumbnail" />
          </div>
          <div class="min-w-0 flex-1">
            <div class="text-[13px] font-semibold text-foreground">{pet.displayName}</div>
            <div class="mt-0.5 text-xs text-muted-foreground line-clamp-2 leading-relaxed">
              {pet.description}
            </div>
          </div>
          <div class="shrink-0">
            {#if isSelected}
              <span class="rounded-md bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary"
                >已选用</span
              >
            {:else}
              <span
                class="rounded-md border border-border/70 px-2 py-1 text-[11px] font-medium text-muted-foreground hover:text-foreground"
                >选用</span
              >
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </SettingsSection>

  <!-- 3. GrokBot 专属定制 (当选中 GrokBot 时) -->
  {#if petSettings.id === "grokbot"}
    <SettingsSection title="GrokBot 外观定制" description="自定义果冻感 SVG 宠物的表情形态与配件">
      <GrokBotCustomizer settings={petSettings} onChange={(patch) => savePet(patch)} />
    </SettingsSection>
  {/if}

  <!-- 4. 自定义宠物导入 -->
  <SettingsSection title="自定义宠物扩展" description="导入自定义序列帧精灵表或动图宠物">
    {#snippet action()}
      <div class="flex items-center gap-2">
        <button
          type="button"
          class="rounded-lg border border-border/70 bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
          onclick={openCustomPetsFolder}
        >
          打开素材目录
        </button>
        <button
          type="button"
          class="rounded-lg border border-border/70 bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
          onclick={() => {
            petCreateError = "";
            petAddOpen = true;
          }}
        >
          + 导入宠物
        </button>
      </div>
    {/snippet}

    <SettingsCard>
      {#if customPets.length === 0}
        <div class="px-5 py-6 text-center text-xs text-muted-foreground">
          暂无自定义宠物。点击右上角 "+ 导入宠物" 添加你喜欢的 Sprite 动画。
        </div>
      {:else}
        {#each customPets as pet (pet.id)}
          <SettingsRow title={pet.displayName} description={pet.description}>
            {#snippet icon()}
              <div class="h-10 w-10 overflow-hidden rounded-lg bg-muted/30">
                <PetPreview petId={pet.id} size="thumbnail" />
              </div>
            {/snippet}
            <button
              type="button"
              class="rounded-md border border-border/70 px-2.5 py-1 text-xs font-medium transition-colors {pet.id ===
              petSettings.id
                ? 'bg-primary/10 text-primary'
                : 'hover:bg-accent text-muted-foreground'}"
              disabled={pet.id === petSettings.id}
              onclick={() => savePet({ id: pet.id })}
            >
              {pet.id === petSettings.id ? "已选用" : "选用"}
            </button>
          </SettingsRow>
        {/each}
      {/if}
    </SettingsCard>
  </SettingsSection>

  <!-- Add custom pet modal -->
  <Modal bind:open={petAddOpen} title={t("settings_pet_addTitle")} closeable={!petCreateBusy}>
    <div class="space-y-4">
      <p class="text-xs text-muted-foreground">
        {t("settings_pet_addDesc")}
      </p>

      <div class="grid grid-cols-2 gap-2">
        <button
          type="button"
          class="rounded-xl border p-3 text-left transition-colors {petAddMode === 'image'
            ? 'border-primary bg-primary/[0.05]'
            : 'border-border hover:bg-muted/40'}"
          onclick={() => (petAddMode = "image")}
        >
          <div class="font-medium text-xs text-foreground">单图静态形象</div>
          <div class="mt-0.5 text-[10px] text-muted-foreground">
            上传单张 PNG/GIF 作为常驻宠物形象
          </div>
        </button>
        <button
          type="button"
          class="rounded-xl border p-3 text-left transition-colors {petAddMode === 'zip'
            ? 'border-primary bg-primary/[0.05]'
            : 'border-border hover:bg-muted/40'}"
          onclick={() => (petAddMode = "zip")}
        >
          <div class="font-medium text-xs text-foreground">动作动画包 (ZIP)</div>
          <div class="mt-0.5 text-[10px] text-muted-foreground">
            上传包含完整帧动画的 ZIP 压缩包
          </div>
        </button>
      </div>

      <div class="flex items-center gap-3">
        <Button
          variant="outline"
          size="sm"
          onclick={petAddMode === "image" ? choosePetImage : choosePetZip}
        >
          {petAddMode === "image" ? "选择图片文件" : "选择 ZIP 动画包"}
        </Button>
        <span class="text-xs text-muted-foreground truncate max-w-[200px]">
          {petAddMode === "image" ? petImageName || "未选择文件" : petZipName || "未选择文件"}
        </span>
      </div>

      <div class="space-y-2 text-xs">
        <div>
          <label class="block text-[11px] text-muted-foreground mb-1">标识符 (Slug)</label>
          <input
            type="text"
            class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
            placeholder="my-cool-pet"
            bind:value={petForm.slug}
          />
        </div>
        <div>
          <label class="block text-[11px] text-muted-foreground mb-1">显示名称</label>
          <input
            type="text"
            class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
            placeholder="我的宠物"
            bind:value={petForm.displayName}
          />
        </div>
        <div>
          <label class="block text-[11px] text-muted-foreground mb-1">简介</label>
          <input
            type="text"
            class="h-8 w-full rounded-lg border border-border/70 bg-background px-2.5 text-xs text-foreground focus:border-primary focus:outline-none"
            placeholder="轻量可爱的伴侣"
            bind:value={petForm.description}
          />
        </div>
      </div>

      {#if petCreateError}
        <div class="rounded-lg bg-rose-500/10 p-2.5 text-xs text-rose-600 dark:text-rose-400">
          {petCreateError}
        </div>
      {/if}

      <div class="flex items-center justify-end gap-2 pt-2">
        <Button variant="ghost" size="sm" onclick={resetPetCreateForm}>取消</Button>
        <Button
          size="sm"
          disabled={petCreateBusy || !(petAddMode === "image" ? petImageBase64 : petZipBase64)}
          onclick={submitCustomPet}
        >
          {petCreateBusy ? "创建中..." : "确认导入"}
        </Button>
      </div>
    </div>
  </Modal>
</div>
