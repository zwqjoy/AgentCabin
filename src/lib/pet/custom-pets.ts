import { getTransport } from "$lib/transport";
import type { PetMode } from "./types";

export type PetAnimationUrls = Record<PetMode, string>;

export interface CustomPetOption {
  source: "custom";
  id: string;
  displayName: string;
  description: string;
  animationUrls: PetAnimationUrls;
  singleFrame: boolean;
  custom: true;
}

type CustomPetPayload = Omit<CustomPetOption, "source">;

export async function listCustomPets(): Promise<CustomPetOption[]> {
  const transport = getTransport();
  if (!transport.isDesktop()) return [];
  const pets = await transport.invoke<CustomPetPayload[]>("list_custom_pets");
  return pets.map((pet) => ({ source: "custom", ...pet }));
}

export async function inspectCustomPetZip(zipBase64: string): Promise<PetAnimationUrls> {
  const transport = getTransport();
  return transport.invoke<PetAnimationUrls>("inspect_custom_pet_zip", { zipBase64 });
}

export async function inspectCustomPetImage(
  imageBase64: string,
  imageName: string,
): Promise<string> {
  const transport = getTransport();
  return transport.invoke<string>("inspect_custom_pet_image", { imageBase64, imageName });
}

export async function createCustomPet(input: {
  slug: string;
  displayName: string;
  description: string;
  zipBase64?: string;
  imageBase64?: string;
  imageName?: string;
}): Promise<CustomPetOption> {
  const transport = getTransport();
  const pet = await transport.invoke<CustomPetPayload>("create_custom_pet", { input });
  return { source: "custom", ...pet };
}
