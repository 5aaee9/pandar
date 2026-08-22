"use client";

import { useRef, useState } from "react";

import type { ArtifactMetadata, Job } from "./dashboard-types";
import {
  maxArtifactBytes,
  type DispatchArtifactState,
  type MetadataPreviewState,
} from "./dispatch-artifact-field";
import { apiIdSegment } from "./api-path";

export function useDispatchArtifact(
  selectedTenant: { id: string } | null,
  sourceJob?: Job | null,
) {
  const [plateId, setPlateId] = useState<number | null>(() =>
    sourcePlateId(sourceJob),
  );
  const [artifact, setArtifact] = useState<DispatchArtifactState>(() =>
    sourceArtifact(sourceJob),
  );
  const [metadataPreview, setMetadataPreview] = useState<MetadataPreviewState>(
    () => sourceMetadataPreview(sourceJob),
  );
  const previewRequestRef = useRef(0);

  const previewArtifact = async (file: File) => {
    if (!selectedTenant) {
      setPlateId(null);
      setMetadataPreview({ state: "idle", metadata: null });
      return;
    }

    const formData = new FormData();
    formData.set("filename", file.name);
    formData.set("content_type", file.type || "application/octet-stream");
    formData.set("file", file);
    const requestId = previewRequestRef.current + 1;
    previewRequestRef.current = requestId;
    setMetadataPreview({ state: "loading", metadata: null });
    const isStale = () => requestId !== previewRequestRef.current;

    try {
      const response = await fetch(metadataPreviewPath(selectedTenant.id), {
        method: "POST",
        body: formData,
      });
      if (isStale()) {
        return;
      }
      if (!response.ok) {
        setPlateId(1);
        setMetadataPreview({ state: "error", metadata: null });
        return;
      }
      const body = (await response.json()) as {
        metadata?: ArtifactMetadata | null;
      };
      if (isStale()) {
        return;
      }
      const defaultPlate = body.metadata?.plates.find(
        (plate) => plate.plate_id === body.metadata?.default_plate_id,
      );
      setPlateId(
        defaultPlate?.plate_id ?? body.metadata?.plates[0]?.plate_id ?? 1,
      );
      setMetadataPreview(
        body.metadata
          ? { state: "ready", metadata: body.metadata }
          : { state: "unavailable", metadata: null },
      );
    } catch {
      if (isStale()) {
        return;
      }
      setPlateId(1);
      setMetadataPreview({ state: "error", metadata: null });
    }
  };

  const selectArtifact = (file: File | null) => {
    if (!file) {
      previewRequestRef.current += 1;
      setPlateId(null);
      setArtifact({ file: null, size: 0, state: "idle" });
      setMetadataPreview({ state: "idle", metadata: null });
      return;
    }

    if (file.size > maxArtifactBytes) {
      previewRequestRef.current += 1;
      setPlateId(null);
      setArtifact({ file, size: file.size, state: "too_large" });
      setMetadataPreview({ state: "idle", metadata: null });
      return;
    }

    setPlateId(null);
    setArtifact({ file, size: file.size, state: "ready" });
    void previewArtifact(file);
  };

  return {
    artifact,
    metadataPreview,
    plateId,
    selectArtifact,
    setPlateId,
  };
}

function metadataPreviewPath(tenantId: string) {
  return `/api/tenants/${apiIdSegment(tenantId, "tenant_id")}/artifact-metadata-preview`;
}

function sourcePlateId(sourceJob?: Job | null) {
  if (!sourceJob) return null;
  const metadata = sourceJob.artifact.metadata;
  return (
    metadata?.plates.find(
      (plate) => plate.plate_id === metadata.default_plate_id,
    )?.plate_id ??
    metadata?.plates[0]?.plate_id ??
    1
  );
}

function sourceArtifact(sourceJob?: Job | null): DispatchArtifactState {
  return sourceJob
    ? { file: null, size: sourceJob.artifact.size_bytes, state: "ready" }
    : { file: null, size: 0, state: "idle" };
}

function sourceMetadataPreview(sourceJob?: Job | null): MetadataPreviewState {
  if (!sourceJob) return { state: "idle", metadata: null };
  return sourceJob.artifact.metadata
    ? { state: "ready", metadata: sourceJob.artifact.metadata }
    : { state: "unavailable", metadata: null };
}
