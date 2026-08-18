export type ApiMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export type ApiRouteContract = {
  method: ApiMethod;
  path: string;
};

export type ApiErrorPayload = {
  detail?: string | { msg?: string; [key: string]: unknown }[];
  message?: string;
  error?: string;
};

export type PaginatedResponse<T> = {
  items: T[];
  total?: number;
  page?: number;
  page_size?: number;
};

export type ApiResult<T> = {
  ok: boolean;
  data?: T;
  error?: ApiErrorPayload | string;
};
