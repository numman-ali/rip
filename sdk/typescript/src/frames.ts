export type RipEventFrame = {
  type: string;
  [key: string]: unknown;
};

export type OutputTextDeltaFrame = RipEventFrame & {
  type: "output_text_delta";
  delta: string;
};

