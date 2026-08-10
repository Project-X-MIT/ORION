import { forwardRef } from "react";

import { Input } from "../ui/Input";
import type { InputProps } from "../ui/Input";

export type TextFieldProps = InputProps;

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(function TextField(props, ref) {
  return <Input ref={ref} type="text" {...props} />;
});
