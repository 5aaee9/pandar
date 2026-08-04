export type PrinterNozzleSystem = {
  nozzle: {
    exist?: number | null;
    state?: number | null;
    src_id?: number | null;
    tar_id?: number | null;
    info: Array<{
      id: number;
      diameter: number;
      type: string;
      stat?: number | null;
      fila_id?: string | null;
      wear?: number | null;
      p_t?: number | null;
      color_m?: string | null;
    }>;
  };
  holder?: {
    stat?: number | null;
    pos?: number | null;
    info?: number | null;
  } | null;
};
