import * as fhir_r4 from 'fhir/r4';
import { Resource, OperationOutcome, Bundle, CapabilityStatement, BundleEntry, Parameters } from 'fhir/r4';

type TokenProvider = () => Promise<string> | string;
interface BearerTokenAuth {
    token: string;
}
interface TokenProviderAuth {
    tokenProvider: TokenProvider;
}
type AuthConfig = BearerTokenAuth | TokenProviderAuth;
interface FhirClientConfig {
    baseUrl: string;
    auth?: AuthConfig;
    timeout?: number;
    headers?: Record<string, string>;
}
interface RequestOptions {
    headers?: Record<string, string>;
    signal?: AbortSignal;
    ifMatch?: string;
    ifNoneMatch?: string;
    ifNoneExist?: string;
    ifModifiedSince?: Date;
    prefer?: "return=minimal" | "return=representation" | "return=OperationOutcome";
}
interface CreateOptions extends RequestOptions {
    prefer?: "return=minimal" | "return=representation" | "return=OperationOutcome";
}
interface UpdateOptions extends RequestOptions {
    ifMatch?: string;
}
interface DeleteOptions extends RequestOptions {
    ifMatch?: string;
}
interface ReadOptions extends RequestOptions {
    summary?: "true" | "text" | "data" | "count" | "false";
    elements?: string[];
}
interface HistoryOptions extends RequestOptions {
    count?: number;
    since?: Date | string;
    at?: Date | string;
}
interface OperationOptions extends RequestOptions {
    method?: "GET" | "POST";
}
type ResourceType = Resource["resourceType"];

interface ResponseMeta {
    status: number;
    etag?: string;
    lastModified?: Date;
    location?: string;
    contentLocation?: string;
}
interface FhirResponse<T extends Resource = Resource> {
    resource: T;
    meta: ResponseMeta;
}
interface BundleResponse {
    bundle: Bundle;
    meta: ResponseMeta;
}
interface DeleteResponse {
    operationOutcome?: OperationOutcome;
    meta: ResponseMeta;
}
interface CapabilitiesResponse {
    capabilityStatement: CapabilityStatement;
    meta: ResponseMeta;
}

interface JsonPatchAdd {
    op: "add";
    path: string;
    value: unknown;
}
interface JsonPatchRemove {
    op: "remove";
    path: string;
}
interface JsonPatchReplace {
    op: "replace";
    path: string;
    value: unknown;
}
interface JsonPatchMove {
    op: "move";
    from: string;
    path: string;
}
interface JsonPatchCopy {
    op: "copy";
    from: string;
    path: string;
}
interface JsonPatchTest {
    op: "test";
    path: string;
    value: unknown;
}
type JsonPatchOperation = JsonPatchAdd | JsonPatchRemove | JsonPatchReplace | JsonPatchMove | JsonPatchCopy | JsonPatchTest;

interface SearchParams {
    [key: string]: string | string[] | undefined;
}
interface SearchOptions {
    headers?: Record<string, string>;
    signal?: AbortSignal;
}
interface SearchResultMeta extends ResponseMeta {
    total?: number;
    link?: Bundle["link"];
}
interface SearchResult<T extends Resource = Resource> {
    bundle: Bundle;
    resources: T[];
    total?: number;
    meta: SearchResultMeta;
    hasNextPage(): boolean;
    hasPrevPage(): boolean;
    nextPage(): Promise<SearchResult<T>>;
    prevPage(): Promise<SearchResult<T>>;
}
type SearchModifier = "exact" | "contains" | "text" | "in" | "not-in" | "below" | "above" | "of-type" | "missing" | "not" | "identifier";
type SearchPrefix = "eq" | "ne" | "gt" | "lt" | "ge" | "le" | "sa" | "eb" | "ap";
interface SearchParameter {
    name: string;
    value: string;
    modifier?: SearchModifier;
}

type ExecuteFn$1 = (resourceType: string, params: URLSearchParams, options?: SearchOptions) => Promise<{
    bundle: Bundle;
    meta: ResponseMeta;
}>;
type ExecuteUrlFn = (url: string, options?: SearchOptions) => Promise<{
    bundle: Bundle;
    meta: ResponseMeta;
}>;
declare class SearchBuilder<T extends Resource = Resource> {
    private readonly resourceType;
    private readonly params;
    private readonly executeFn;
    private readonly executeUrlFn;
    private searchOptions?;
    constructor(resourceType: string, executeFn: ExecuteFn$1, executeUrlFn: ExecuteUrlFn);
    where(name: string, value: string | string[]): this;
    whereExact(name: string, value: string): this;
    whereContains(name: string, value: string): this;
    whereText(name: string, value: string): this;
    whereMissing(name: string, isMissing?: boolean): this;
    whereNot(name: string, value: string): this;
    whereBelow(name: string, value: string): this;
    whereAbove(name: string, value: string): this;
    whereIn(name: string, valueSetUrl: string): this;
    whereNotIn(name: string, valueSetUrl: string): this;
    whereOfType(name: string, system: string, code: string): this;
    whereIdentifier(name: string, system: string, value: string): this;
    withModifier(name: string, modifier: SearchModifier, value: string): this;
    include(value: string): this;
    includeIterate(value: string): this;
    revinclude(value: string): this;
    revincludeIterate(value: string): this;
    sort(field: string): this;
    count(n: number): this;
    offset(n: number): this;
    summary(mode: "true" | "text" | "data" | "count" | "false"): this;
    elements(...elements: string[]): this;
    contained(mode: "true" | "false" | "both"): this;
    containedType(mode: "container" | "contained"): this;
    total(mode: "none" | "estimate" | "accurate"): this;
    withOptions(options: SearchOptions): this;
    getParams(): URLSearchParams;
    execute(): Promise<SearchResult<T>>;
    private createSearchResult;
    private extractResources;
}

type BundleType = "batch" | "transaction";
interface BundleEntryOptions {
    fullUrl?: string;
    ifMatch?: string;
    ifNoneMatch?: string;
    ifNoneExist?: string;
    ifModifiedSince?: string;
}
interface BundleResult {
    bundle: Bundle;
    meta: ResponseMeta;
    getResource<T extends Resource>(index: number): T | undefined;
    getResourceByFullUrl<T extends Resource>(fullUrl: string): T | undefined;
    isSuccess(index: number): boolean;
    getStatus(index: number): number | undefined;
    getLocation(index: number): string | undefined;
}
type ExecuteFn = (bundle: Bundle) => Promise<{
    bundle: Bundle;
    meta: ResponseMeta;
}>;
declare class BundleBuilder {
    private readonly type;
    private readonly entries;
    private readonly executeFn;
    constructor(type: BundleType, executeFn: ExecuteFn);
    create<T extends Resource>(resource: T, options?: BundleEntryOptions): this;
    update<T extends Resource>(resource: T, options?: BundleEntryOptions): this;
    conditionalUpdate<T extends Resource>(resource: T, searchParams: string, options?: BundleEntryOptions): this;
    patch(resourceType: string, id: string, operations: JsonPatchOperation[], options?: BundleEntryOptions): this;
    delete(resourceType: string, id: string, options?: BundleEntryOptions): this;
    conditionalDelete(resourceType: string, searchParams: string, options?: BundleEntryOptions): this;
    read(resourceType: string, id: string, options?: BundleEntryOptions): this;
    search(resourceType: string, params: string, options?: BundleEntryOptions): this;
    addEntry(entry: BundleEntry): this;
    getBundle(): Bundle;
    execute(): Promise<BundleResult>;
    private createBundleResult;
}

declare class FhirClient {
    private readonly baseUrl;
    private readonly auth?;
    private readonly timeout;
    private readonly defaultHeaders;
    constructor(config: FhirClientConfig);
    private getAuthHeader;
    private request;
    private mergeSignals;
    private parseResponseMeta;
    createResource<T extends Resource>(resource: T, options?: CreateOptions): Promise<FhirResponse<T>>;
    readResource<T extends Resource>(resourceType: string, id: string, options?: ReadOptions): Promise<FhirResponse<T>>;
    vreadResource<T extends Resource>(resourceType: string, id: string, versionId: string, options?: RequestOptions): Promise<FhirResponse<T>>;
    updateResource<T extends Resource>(resource: T, options?: UpdateOptions): Promise<FhirResponse<T>>;
    patchResource<T extends Resource>(resourceType: string, id: string, operations: JsonPatchOperation[], options?: UpdateOptions): Promise<FhirResponse<T>>;
    deleteResource(resourceType: string, id: string, options?: DeleteOptions): Promise<DeleteResponse>;
    conditionalCreate<T extends Resource>(resource: T, searchParams: string, options?: CreateOptions): Promise<FhirResponse<T>>;
    conditionalUpdate<T extends Resource>(resource: T, searchParams: string, options?: UpdateOptions): Promise<FhirResponse<T>>;
    conditionalDelete(resourceType: string, searchParams: string, options?: DeleteOptions): Promise<DeleteResponse>;
    search<T extends Resource>(resourceType: string): SearchBuilder<T>;
    private executeSearch;
    private executeSearchUrl;
    batch(): BundleBuilder;
    transaction(): BundleBuilder;
    private executeBundle;
    operation<T extends Resource | Parameters = Parameters>(name: string, params?: Parameters, options?: OperationOptions): Promise<FhirResponse<T>>;
    typeOperation<T extends Resource | Parameters = Parameters>(resourceType: string, name: string, params?: Parameters, options?: OperationOptions): Promise<FhirResponse<T>>;
    instanceOperation<T extends Resource | Parameters = Parameters>(resourceType: string, id: string, name: string, params?: Parameters, options?: OperationOptions): Promise<FhirResponse<T>>;
    private parametersToSearchParams;
    history(resourceType: string, id: string, options?: HistoryOptions): Promise<BundleResponse>;
    typeHistory(resourceType: string, options?: HistoryOptions): Promise<BundleResponse>;
    systemHistory(options?: HistoryOptions): Promise<BundleResponse>;
    private appendHistoryParams;
    capabilities(options?: RequestOptions): Promise<CapabilitiesResponse>;
}

interface ClientCredentialsConfig {
    clientId: string;
    clientSecret: string;
    issuer?: string;
    tokenEndpoint?: string;
    scope?: string;
}
declare class ClientCredentialsAuth {
    private readonly config;
    private cachedToken?;
    private tokenExpiresAt?;
    private tokenEndpoint?;
    private tokenPromise?;
    constructor(config: ClientCredentialsConfig);
    private discoverTokenEndpoint;
    private fetchToken;
    private isTokenValid;
    getToken(): Promise<string>;
    tokenProvider(): TokenProvider;
    clearCache(): void;
}

interface SmartConfiguration {
    issuer: string;
    authorization_endpoint: string;
    token_endpoint: string;
    jwks_uri: string;
    capabilities?: string[];
    registration_endpoint?: string;
    introspection_endpoint?: string;
    revocation_endpoint?: string;
    userinfo_endpoint?: string;
}
interface SmartAuthConfig {
    fhirBaseUrl: string;
    clientId: string;
    redirectUri: string;
    scope?: string;
    launch?: string;
    state?: string;
    storage?: SmartAuthStorage;
}
interface SmartAuthStorage {
    get(key: string): string | null;
    set(key: string, value: string): void;
    remove(key: string): void;
}
interface TokenResponse {
    access_token: string;
    token_type: string;
    expires_in?: number;
    refresh_token?: string;
    scope?: string;
    patient?: string;
    encounter?: string;
    need_patient_banner?: boolean;
    smart_style_url?: string;
}
/**
 * SMART on FHIR Authorization Code Flow with PKCE
 *
 * Handles OAuth2 authorization code flow with PKCE for secure authentication.
 * Supports automatic token refresh and seamless integration with FhirClient.
 */
declare class SmartAuth {
    private readonly config;
    private readonly storage;
    private smartConfig;
    private tokenData;
    private tokenPromise;
    constructor(config: SmartAuthConfig);
    /**
     * Discover SMART configuration from the FHIR server
     */
    discover(): Promise<SmartConfiguration>;
    /**
     * Generate PKCE code verifier and challenge
     */
    private generatePKCE;
    /**
     * Generate random state for CSRF protection
     */
    private generateState;
    /**
     * Initiate authorization flow
     * In browser: redirects to authorization server
     * In Node.js: returns authorization URL
     */
    authorize(): Promise<string>;
    /**
     * Handle authorization callback and exchange code for token
     */
    handleCallback(code: string, state: string, error?: string): Promise<TokenResponse>;
    /**
     * Refresh access token using refresh token
     */
    private refreshToken;
    /**
     * Set token data and store in storage
     */
    private setToken;
    /**
     * Restore token from storage
     */
    private restoreToken;
    /**
     * Check if user is authenticated
     */
    isAuthenticated(): boolean;
    /**
     * Get current access token, refreshing if necessary
     */
    getToken(): Promise<string>;
    /**
     * Get current patient ID from token (if available)
     */
    getPatientId(): string | undefined;
    /**
     * Get current scopes from token
     */
    getScopes(): string[];
    /**
     * Clear token and logout
     */
    logout(): void;
    private clearToken;
    /**
     * Create token provider for FhirClient integration
     */
    tokenProvider(): TokenProvider;
    /**
     * Create default storage implementation
     */
    private createDefaultStorage;
}

declare class FhirError extends Error {
    readonly status: number;
    readonly operationOutcome?: OperationOutcome;
    readonly response?: Response;
    constructor(message: string, status: number, operationOutcome?: OperationOutcome, response?: Response);
    get issues(): fhir_r4.OperationOutcomeIssue[];
    static fromResponse(status: number, operationOutcome?: OperationOutcome, response?: Response): FhirError;
}
declare class NotFoundError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class GoneError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class ConflictError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class PreconditionFailedError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class ValidationError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class UnprocessableEntityError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class AuthenticationError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class ForbiddenError extends FhirError {
    constructor(message: string, operationOutcome?: OperationOutcome, response?: Response);
}
declare class NetworkError extends Error {
    readonly cause?: Error;
    constructor(message: string, cause?: Error);
}
declare class TimeoutError extends Error {
    constructor(message?: string);
}

export { type AuthConfig, AuthenticationError, type BearerTokenAuth, BundleBuilder, type BundleEntryOptions, type BundleResponse, type BundleResult, type CapabilitiesResponse, ClientCredentialsAuth, type ClientCredentialsConfig, ConflictError, type CreateOptions, type DeleteOptions, type DeleteResponse, FhirClient, type FhirClientConfig, FhirError, type FhirResponse, ForbiddenError, GoneError, type HistoryOptions, type JsonPatchAdd, type JsonPatchCopy, type JsonPatchMove, type JsonPatchOperation, type JsonPatchRemove, type JsonPatchReplace, type JsonPatchTest, NetworkError, NotFoundError, type OperationOptions, PreconditionFailedError, type ReadOptions, type RequestOptions, type ResourceType, type ResponseMeta, SearchBuilder, type SearchModifier, type SearchOptions, type SearchParameter, type SearchParams, type SearchPrefix, type SearchResult, type SearchResultMeta, SmartAuth, type SmartAuthConfig, type SmartAuthStorage, type SmartConfiguration, TimeoutError, type TokenProvider, type TokenProviderAuth, UnprocessableEntityError, type UpdateOptions, ValidationError };
