import type { OperationOutcome } from "fhir/r4";

export class FhirError extends Error {
  readonly status: number;
  readonly operationOutcome?: OperationOutcome;
  readonly response?: Response;

  constructor(
    message: string,
    status: number,
    operationOutcome?: OperationOutcome,
    response?: Response
  ) {
    super(message);
    this.name = "FhirError";
    this.status = status;
    this.operationOutcome = operationOutcome;
    this.response = response;
    Object.setPrototypeOf(this, new.target.prototype);
  }

  get issues() {
    return this.operationOutcome?.issue ?? [];
  }

  static fromResponse(
    status: number,
    operationOutcome?: OperationOutcome,
    response?: Response
  ): FhirError {
    const message = operationOutcome?.issue?.[0]?.diagnostics
      ?? operationOutcome?.issue?.[0]?.details?.text
      ?? `FHIR request failed with status ${status}`;

    switch (status) {
      case 400:
        return new ValidationError(message, operationOutcome, response);
      case 401:
        return new AuthenticationError(message, operationOutcome, response);
      case 403:
        return new ForbiddenError(message, operationOutcome, response);
      case 404:
        return new NotFoundError(message, operationOutcome, response);
      case 409:
        return new ConflictError(message, operationOutcome, response);
      case 410:
        return new GoneError(message, operationOutcome, response);
      case 412:
        return new PreconditionFailedError(message, operationOutcome, response);
      case 422:
        return new UnprocessableEntityError(message, operationOutcome, response);
      default:
        return new FhirError(message, status, operationOutcome, response);
    }
  }
}

export class NotFoundError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 404, operationOutcome, response);
    this.name = "NotFoundError";
  }
}

export class GoneError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 410, operationOutcome, response);
    this.name = "GoneError";
  }
}

export class ConflictError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 409, operationOutcome, response);
    this.name = "ConflictError";
  }
}

export class PreconditionFailedError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 412, operationOutcome, response);
    this.name = "PreconditionFailedError";
  }
}

export class ValidationError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 400, operationOutcome, response);
    this.name = "ValidationError";
  }
}

export class UnprocessableEntityError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 422, operationOutcome, response);
    this.name = "UnprocessableEntityError";
  }
}

export class AuthenticationError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 401, operationOutcome, response);
    this.name = "AuthenticationError";
  }
}

export class ForbiddenError extends FhirError {
  constructor(message: string, operationOutcome?: OperationOutcome, response?: Response) {
    super(message, 403, operationOutcome, response);
    this.name = "ForbiddenError";
  }
}

export class NetworkError extends Error {
  readonly cause?: Error;

  constructor(message: string, cause?: Error) {
    super(message);
    this.name = "NetworkError";
    this.cause = cause;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class TimeoutError extends Error {
  constructor(message: string = "Request timed out") {
    super(message);
    this.name = "TimeoutError";
    Object.setPrototypeOf(this, new.target.prototype);
  }
}
