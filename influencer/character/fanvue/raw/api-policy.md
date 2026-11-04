> For clean Markdown of any page, append .md to the page URL.
> For a complete documentation index, see https://legal.fanvue.com/llms.txt.
> For AI client integration (Claude Code, Cursor, etc.), connect to the MCP server at https://legal.fanvue.com/_mcp/server.

# API Access & Usage Policy

This API Access & Usage Policy (the "Policy") governs use of Fanvue's application programming interfaces (the "API"). It applies to every **App** that accesses the API and to every **App Builder** that builds or operates one, whether or not the **App** is listed on the **Fanvue App Store**.

Apps listed on the **Fanvue App Store** must also meet the [**App Store Listing Requirements**](https://api.fanvue.com/docs/app-store/listing-requirements). Those requirements apply in addition to this Policy and do not replace it: an **App** that is not listed, or not yet listed, must still comply with this Policy and with Fanvue's content policies.

This Policy supplements the [**General Terms & Conditions**](/terms-conditions) and other applicable [**Fanvue Policies**](/welcome). If there is any conflict between this Policy and the General Terms, the terms of this Policy shall prevail for the specific subject matter it governs.

All defined terms used herein shall have the meaning assigned in the [**General Terms & Conditions**](/terms-conditions), unless otherwise stated.

---

## DEFINITIONS

"**App**" (also "**Third-Party Application**", "**OAuth Application**" or "**Developer Application**") means any application, integration, script, service or tool registered in the Fanvue Developer Area that requests access to Fanvue accounts through OAuth authorisation, whether or not it is listed on the **Fanvue App Store**.

"**App Builder**" means the individual or organisation that registers, operates and controls an **App**, including its authorised team members and any service providers acting on its behalf.

"**Fanvue App Store**" means the section of the Platform through which approved **Apps** are made available to Users.

"**Access Token**" means a short-lived OAuth 2.0 credential issued by Fanvue when a User authorises an **App**, which the **App** sends with each API request to act on that User's behalf. "**Refresh Token**" means the credential used to obtain a new **Access Token** without the User authorising the **App** again.

"**Client Credentials**" means an **App**'s client secret, together with any **Access Tokens** and **Refresh Tokens** issued to it. **Client Credentials** are confidential.

"**App Configuration**" means the settings registered for an **App**, including its client ID, redirect URIs and webhook endpoint URLs. **App Configuration** is not confidential, but determines where Fanvue sends authorisation codes and event data, so it must only point to services the **App Builder** controls (see [**Section 5.1**](#51-prohibited-conduct)).

"**Scopes**" means the permissions an **App** requests and a User grants at authorisation, which determine which endpoints and operations the **App** may access on that User's behalf.

"**API Version**" means the version identifier for the API, sent in the `X-Fanvue-API-Version` header. The value must follow the format `YYYY-MM-DD` (e.g. `2025-06-26`).

"**Rate Limit**" means the maximum number of requests an **App** may make within a defined window, as set out in Section 6.

"**Supported Version**" means an API Version that Fanvue currently supports. Fanvue may deprecate or sunset versions.

---

## 1. ACCESS & AUTHORISATION

### 1.1 Authentication

1. All access to the API is through OAuth 2.0 (with PKCE). Fanvue does not issue API keys or personal access tokens.
2. Every API request must carry a valid **Access Token** in the `Authorization: Bearer` header. **Access Tokens** are only issued when a User authorises the **App** through Fanvue's consent screen.
3. On successful authentication, the request is associated with the User who authorised the **App** and the **Scopes** that User granted.

### 1.2 Authorisation

1. Endpoints declare required **Scopes**. Access is granted only if the **Access Token** carries the required **Scopes**.
2. Requests without valid credentials or sufficient permissions will return a `401 Unauthorized` or `403 Forbidden` response status code, as appropriate.
3. Users may revoke an **App**'s access at any time, after which its **Access Tokens** and **Refresh Tokens** for that User stop working.

### 1.3 API Versioning

1. Requests should include the header `X-Fanvue-API-Version` in the format `YYYY-MM-DD`.
2. Fanvue may reject unsupported or sunset versions and require an upgrade to a Supported Version.
3. Version lifecycle follows: Active → Deprecated → Sunset.
4. Deprecation is communicated via RFC 8594 standard headers:
   1. `Deprecation`: Indicates when the version was deprecated.
   2. `Sunset`: Specifies when the version will stop working.
   3. `X-Fanvue-API-Next-Version`: Recommends the migration path.

## 2. APP REGISTRATION & CREDENTIALS

1. Each **App** must be registered in the Fanvue Developer Area by the **App Builder** who will operate and control it. The **App Builder** is responsible for the **App** and everything done through it.
2. **Client Credentials** must be kept confidential and stored server-side. They must not be shared with anyone other than the **App Builder**, embedded in client-side code, or exposed publicly.
3. A client secret is shown once, at creation, and can be reset at any time in the Developer Area. Resetting a secret invalidates the old one immediately.
4. **App Configuration** must only point to domains and services that the **App Builder** owns or controls, directly or through a service provider acting on its behalf.
5. Fanvue may record API usage metadata (e.g. client identifier, method, path, timestamps) for security, audit, and operational purposes in accordance with our [**Privacy Policy**](/privacy-policy).

## 3. TECHNICAL REQUIREMENTS

1. Required headers on secured endpoints:
   1. `Authorization: Bearer <access_token>`: a valid, unexpired **Access Token**.
   2. `X-Fanvue-API-Version`: the requested API Version (e.g. `2025-06-26`).
2. Requests must be made over HTTPS. Plain HTTP is not permitted.
3. Webhook deliveries must be verified using the signature Fanvue provides before they are acted on.
4. Clients must handle error responses appropriately:
   1. `400 Bad Request`: Invalid request (e.g., unsupported API version, malformed parameters). Check the error message and API documentation for details.
   2. `401 Unauthorised`: Missing or invalid authentication.
   3. `403 Forbidden`: Insufficient permissions for the requested endpoint.
   4. `410 Gone`: API version permanently removed (includes `nextVersion` field).
   5. `429 Too Many Requests`: Rate limit exceeded (includes `Retry-After` header).

## 4. PERMITTED USE

### 4.1 Scope of Access

1. Subject to the **Scopes** a User grants, the API enables operations including:
   1. **Users**: Reading current user details.
   2. **Chats**: Listing chats, retrieving chat messages, creating new chats, sending messages, sending mass messages, deleting messages, and updating chat properties.
   3. **Posts**: Creating new posts.
   4. **Custom Lists**: Listing custom lists and members, creating lists, renaming lists, deleting lists, adding members to lists, and removing members from lists.
   5. **Media & Vault**: Creating multipart upload sessions, obtaining signed URLs for uploads, completing upload sessions, creating vault folders, renaming vault folders, deleting vault folders, adding media to folders, and removing media from folders.
   6. **Tracking Links**: Listing tracking links, creating tracking links, and deleting tracking links.
   7. **Creators**: Reading followers and subscribers.
   8. **Insights**: Reading earnings data, top-spending fans, and subscriber counts.
   9. **Agencies**: Managing creators, creator-specific chat operations (creating chats, sending messages, sending mass messages), creator media uploads, creating posts for creators, managing tracking links for creators, and accessing creator analytics.
   10. **Webhooks**: Subscribing to events relating to the authorising User's account.
2. The API provides both read and write access depending on the **Scopes** granted. Write operations modify data on the Platform and should be used responsibly.
3. Pagination and filtering may apply as documented for each endpoint.
4. Fanvue may limit access to certain endpoints, **Scopes** or features to approved **App Builders**.

### 4.2 Content Policies Apply to All Apps

1. Anything an **App** creates, publishes, sends, uploads or displays on or through the Platform is treated as User Content of the User on whose behalf it acts, and must comply with the [**Community Guidelines**](/community-guidelines), the [**Acceptable Use Policy**](/acceptable-use-policy) and all other Fanvue content policies.
2. This applies to every **App**, whether it is listed on the **Fanvue App Store**, awaiting review, or used privately by a single User.
3. **App Builders** must not design or configure an **App** to bypass, interfere with or evade Fanvue's moderation systems, and must take reasonable steps to prevent their **App** being used to create or distribute prohibited content.
4. Using an **App** does not change who is responsible for content. Creators remain responsible for content published or sent from their account, including content produced by an **App** they have authorised.

### 4.3 AI Disclosure

**Apps** that use artificial intelligence to generate, edit or send content must meet the following requirements, in addition to [**Community Guidelines – Section D (AI-Generated Media)**](/community-guidelines#d-ai-generated-media):

1. **AI-generated media**: Images, video and audio that are generated or materially altered using AI must carry a clear and prominent disclosure that they are AI-generated. Where an **App** publishes or sends such media on a User's behalf, the **App** must apply or preserve that disclosure, and must not remove or obscure it.
2. **AI chat and personas**: **Apps** that generate or send messages on a Creator's behalf using AI, or that operate an AI persona or chatbot, must:
   1. Only operate with the authorisation of the Creator whose account they act for, and allow that Creator to review, pause and switch off AI messaging at any time;
   2. Not be designed or configured to deceive Fans into believing they are interacting live with a real person when they are not, in line with the [**Acceptable Use Policy – Content Authenticity & Quality**](/acceptable-use-policy#3-content-authenticity--quality);
   3. Not deny being AI when a Fan sincerely asks whether they are talking to a real person;
   4. Not generate messages or media that would be prohibited under Fanvue's content policies.
3. **Real people**: **Apps** must not generate, alter or replicate the likeness or voice of a real, identifiable person without that person's documented consent and identity verification, as required by [**Community Guidelines – Section D**](/community-guidelines#d-ai-generated-media).
4. **Minors**: **Apps** must not generate any content depicting, or resembling, a person under 18. Prompt text or disclaimers are not sufficient to overcome visual resemblance to a minor.

## 5. PROHIBITED USE

The restrictions in this Section apply to every **App** and **App Builder**, whether or not the **App** is listed on the **Fanvue App Store**.

### 5.1 Prohibited Conduct

1. Accessing the API other than with an **Access Token** issued to your own **App** through the OAuth flow.
2. Accessing a User's account or data without that User's authorisation, or beyond the **Scopes** they granted.
3. Requesting **Scopes** your **App** does not need for the features it offers.
4. Exceeding Rate Limits, failing to respect `Retry-After` guidance, or attempting to evade Rate Limits (for example, by spreading traffic across multiple **Apps** or accounts).
5. Attempting to subvert authentication, authorisation, versioning controls, rate limiting, moderation or other security controls.
6. Sharing **Client Credentials** with anyone other than the **App Builder**, or exposing them in client-side code, public repositories or elsewhere.
7. Registering an **App** that will be operated or controlled by someone else. Each **App** must be registered by the **App Builder** who operates it. Users who want to use an **App** should connect to it through the **Fanvue App Store** or the standard "Connect with Fanvue" OAuth flow, and never need to register an **App** of their own to do so.
8. Configuring redirect URIs or webhook endpoints that point to domains or services you do not own or control (directly or through a service provider acting on your behalf).
9. Asking, instructing or encouraging other Users to register an **App**, share **Client Credentials**, or configure **App Configuration** for you, as a way of gaining access to their account or data.
10. Asking Users for their Fanvue password, two-factor authentication codes or session cookies, or automating Fanvue's website or apps outside the API, contrary to the [**Acceptable Use Policy – Section 5**](/acceptable-use-policy#5-security-and-technical-constraints).
11. Misrepresenting who built your **App**, what it does, or what data it accesses, including on Fanvue's consent screen.
12. Providing instructions, tutorials, or encouragement to others to use the API in a manner that violates this Policy.
13. Any use contrary to the [**Acceptable Use Policy**](/acceptable-use-policy), the [**Community Guidelines**](/community-guidelines) or this Policy.

### 5.2 Prohibited Apps

Fanvue does not permit **Apps** whose purpose, or a substantial part of whose functionality, falls into any of the categories below. This list sets out common examples and is not exhaustive: any **App** designed to create, distribute or facilitate content or conduct prohibited under Fanvue's policies is also prohibited.

1. **Gambling and games of chance**: Casinos, sports or event betting, lotteries, raffles, sweepstakes, prize draws, loot boxes, or any game of chance played for money or anything of monetary value, including credits, tokens or cryptocurrency.
2. **Nudify and non-consensual sexual imagery**: "Nudify", "undress" or clothing-removal tools, and any deepfake, face-swap or image-editing tool used to create intimate or sexualised imagery of a real person without their documented consent and identity verification.
3. **Content involving minors**: Any **App** that generates, sexualises or facilitates content depicting, or resembling, a person under 18, including "age play" or youthful-appearance features.
4. **Age, identity or verification circumvention**: Tools that help Users bypass, spoof or defeat age verification, identity verification, KYC or liveness checks, including by supplying synthetic or borrowed identities.
5. **Content leaking and paywall circumvention**: Tools that download, archive, scrape, rip or redistribute paid, subscriber-only or pay-per-view content outside the Platform, or that bypass Fanvue's paywalls or access controls.
6. **Data harvesting and de-anonymisation**: Building or selling databases of Fans or Creators, profiling Users for resale, cross-referencing data to identify Fans or Creators in real life, or tracking Users across platforms without their consent.
7. **Credential phishing and account takeover**: **Apps** that collect Fanvue login details, two-factor codes or session cookies, or that trick Users into registering **Apps** or sharing **Client Credentials**.
8. **Fake engagement**: Tools that generate fake accounts, followers, subscribers, likes, views, reviews or ratings, or that operate bots posing as Fans.
9. **Off-platform payments**: Tools that solicit, collect or facilitate payments from Fans for Creator content or interactions outside the Platform's payment systems.
10. **Platform migration**: Tools designed to move Creators' Fans or content to competing creator platforms.
11. **Spam and moderation evasion**: Unsolicited bulk messaging, messaging that evades Fanvue's filters or limits, or tools that disguise prohibited content (for example, through codewords, obfuscation or altered media) to avoid moderation.
12. **Malicious software**: Malware, spyware, stalkerware, cryptominers, or any **App** that accesses a device, account or data without the User's knowledge.
13. **AI training on Fanvue content or data**: Using content, messages or other data obtained through the API to train, fine-tune or otherwise develop AI or machine learning models, unless expressly authorised by Fanvue and by the Users concerned.
14. **Escort services and in-person meetings**: **Apps** that advertise or facilitate escort or other in-person sexual services, or that arrange in-person meetings between Creators and Fans.

## 6. RATE LIMITS & FAIR USAGE

1. Rate Limits are counted per **App** for each User it acts for. Default limits, per-endpoint limits and concurrency limits are published in the [**Fanvue API documentation**](https://api.fanvue.com/docs/authentication/rate-limits) and may change.
2. Responses include standard rate limit headers when applicable:

| **Header**              | **Meaning**                                   |
| ----------------------- | --------------------------------------------- |
| `X-RateLimit-Limit`     | Total request capacity for the current window |
| `X-RateLimit-Remaining` | Requests remaining in the current window      |
| `X-RateLimit-Reset`     | Unix time (seconds) when the window resets    |

3. When limits are exceeded, the API returns `429 Too Many Requests` status code and may include:
   1. `Retry-After` header: seconds until another request may be attempted.
4. Implement exponential backoff and honour `Retry-After` header to avoid throttling.
5. Fanvue may tune limits per route or client and may enforce stricter limits for stability.

## 7. DATA & PRIVACY

This section explains data protection responsibilities in connection with the API. For details on how Fanvue collects, processes, and protects personal data, see our [**Privacy Policy**](/privacy-policy).

### 7.1 App Builder Data Protection Responsibilities

If you build or operate a Third-Party Application that accesses user data through the API, you become a **data controller** (or **data processor**, depending on your relationship with users) for any personal data you access, store, or process. You are responsible for:

1. Having a valid legal basis for processing personal data (e.g., user consent, legitimate interest, or contractual necessity).
2. Implementing appropriate technical and organisational security measures to protect personal data.
3. Not retaining personal data longer than necessary for the purposes disclosed to users.
4. Responding to data subject access requests (DSARs) for any personal data you hold.
5. Notifying Fanvue promptly at [**support@fanvue.com**](mailto:support@fanvue.com) of any data breach involving Fanvue user data.
6. Maintaining your own privacy policy that clearly explains how you collect, use, store, and share personal data obtained through the API.
7. Not sharing personal data with further third parties without appropriate legal basis and safeguards.
8. Complying with all applicable data protection laws, including UK GDPR, EU GDPR, and any other relevant legislation.
9. Processing personal data only for purposes disclosed to users at the time of authorisation.
10. Ensuring any international data transfers you make comply with applicable data protection laws.

Fanvue does not monitor or guarantee App Builder compliance with data protection laws. Users who authorise access to Third-Party Applications do so at their own risk.

### 7.2 User Responsibilities (OAuth Authorisation)

When you authorise a Third-Party Application to access your Fanvue account via OAuth, you acknowledge and accept that:

1. You are granting the Third-Party Application access to your account data as described in the requested Scopes.
2. The App Builder (not Fanvue) controls how your data is used, stored, and shared after authorisation.
3. Fanvue's responsibility for your data ends once it is shared with the Third-Party Application.
4. You are responsible for reviewing the Third-Party Application's permissions and privacy policy before granting access.
5. You can revoke access at any time via Settings → Account → Third Party Apps Consent.
6. You should only authorise Third-Party Applications from developers you trust.
7. Fanvue does not endorse, verify, or guarantee the security or compliance of Third-Party Applications.

### 7.3 Third-Party Users

Third-Party Applications may process data relating to individuals who are not Fanvue users (e.g., recipients of messages sent via integrations, or users of external platforms connected to the API). For such individuals:

1. Fanvue has no direct relationship with these individuals and is not the data controller for their data.
2. App Builders are solely responsible for the lawful collection, processing, and protection of such data.
3. App Builders must ensure compliance with all applicable data protection laws regarding these individuals.
4. Fanvue bears no liability for how App Builders handle data relating to non-Fanvue users.

### 7.4 External Integrations & Liability Limitations

If you use the API to integrate with external services, platforms, or applications (e.g., Slack, Zapier, CRMs, social media platforms, AI services, or other third-party tools), you acknowledge and accept that:

1. **External Terms Compliance**: You are solely responsible for ensuring your use of Fanvue data complies with the terms of service, acceptable use policies, and privacy policies of any external platforms.
2. **Prohibited Use by External Platforms**: If an external service prohibits automated access, certain data uses, or integration with platforms like Fanvue, you are solely responsible for any consequences of non-compliance.
3. **Data Shared Externally**: Once data leaves Fanvue through the API, Fanvue is not liable for its subsequent use, storage, disclosure, or security by you, App Builders, or any external services.
4. **No Guarantee of App Builder Compliance**: Fanvue does not verify, endorse, or guarantee that Third-Party Applications comply with applicable laws, external platform terms, or industry standards.
5. **Downstream Processing**: Fanvue is not responsible for any further sharing, processing, or misuse of data by App Builders, their integrations, or external services.
6. **Third-Party Security**: Fanvue is not responsible for the security practices, data breaches, or failures of external services or App Builders.

You agree to indemnify and hold harmless Fanvue, its officers, directors, employees, and agents from any claims, damages, losses, or expenses (including legal fees) arising from:

1. Your misuse of the API or violation of this Policy.
2. Your violation of any external platform's terms of service or policies.
3. Your failure to comply with applicable data protection laws.
4. Any claims by third parties (including data subjects) relating to your processing of personal data obtained through the API.

### 7.5 Your Data Protection Rights

Under UK and EU data protection law, you have rights regarding your personal data held by Fanvue, including the right to access, rectify, erase, restrict processing, data portability, and object to processing. For full details of your rights and how to exercise them, see our [**Privacy Policy**](/privacy-policy).

For personal data held by App Builders, you must contact the App Builder directly to exercise your rights. Fanvue cannot access, modify, or delete personal data held by third parties.

## 8. INTELLECTUAL PROPERTY

1. The API, its specification, and related documentation are owned by or licensed to Fanvue.
2. This Policy does not grant any IP rights beyond those necessary to consume the API as permitted by the **Scopes** granted to your **App** and the [**General Terms & Conditions**](/terms-conditions).

## 9. RESALE & REUSE

1. Reselling, sublicensing, or redistributing API access, data, or responses to third parties is not permitted unless expressly authorised by Fanvue.
2. Additional restrictions may apply under the [**Acceptable Use Policy**](/acceptable-use-policy) and other Platform terms.

## 10. SECURITY REQUIREMENTS

1. Always use HTTPS and secure, up-to-date TLS configurations.
2. Store **Client Credentials** securely:
   1. Keep client secrets, **Access Tokens** and **Refresh Tokens** server-side, and never expose them to browsers or client-side code.
   2. Use environment variables or secure secrets management systems, and encrypt stored tokens at rest.
   3. Never hardcode credentials in source code or commit them to publicly accessible version control repositories (e.g., GitHub).
   4. Never log **Client Credentials** or personal data.
3. Reset your client secret immediately if compromise is suspected.
4. Monitor API usage for unexpected activity.
5. Handle error responses gracefully and remediate underlying causes before retrying.
6. Do not attempt to bypass security controls or versioning requirements.
7. **Protecting Your Account:**
   1. Never register an **App** at the instruction of another person, service, or chatbot. Legitimate services will have their own registered **App**, and you can connect to them through the **Fanvue App Store** or the "Connect with Fanvue" flow.
   2. Never share your **Client Credentials** with anyone. If a service asks you to register an **App** and send them its credentials, this is likely fraudulent.
   3. Never set an **App**'s redirect URIs or webhook endpoints to addresses someone else has given you.
8. Regularly review connected **Apps** at Settings → Account → Third Party Apps Consent. Revoke access for any **Apps** you do not recognise or no longer use.
9. If you suspect your **Client Credentials** have been compromised or shared:
   1. Immediately revoke third-party access at Settings → Account → Third Party Apps Consent.
   2. Reset the client secret, or delete the affected **App**, in the Developer Area.
   3. Report the incident to Fanvue at [support@fanvue.com](mailto:support@fanvue.com).
10. Be alert to social engineering attacks. Common tactics include:
    1. Chatbots or automated services asking you to register an **App** "for them".
    2. Promises of AI-powered features, automation, or analytics in exchange for registering **Apps**.
    3. Requests to share client secrets or tokens, or to enter someone else's redirect URI or webhook URL.
    4. Pressure to act quickly or share the service with other creators.
11. Report suspected social engineering attempts or fraudulent services to Fanvue immediately.

## 11. TERMINATION, SUSPENSION & VERSIONING

1. Fanvue may suspend or revoke API access, or remove an **App** from the **Fanvue App Store**, at any time for policy breaches, abuse, risk, or operational reasons.
2. Fanvue may revoke an **App**'s **Client Credentials** or tokens, and Users may revoke an **App**'s access to their account at any time; revocation takes immediate effect.
3. Fanvue may deprecate or sunset API Versions. Clients must upgrade to Supported Versions within timelines communicated by Fanvue.
4. Fanvue may apply throttling or other protective measures to preserve Platform integrity.
5. Fanvue may take immediate action, including suspension or termination, against **Apps** and accounts involved in credential sharing, social engineering, facilitating unauthorised access to other users' accounts, or operating an **App** prohibited under [**Section 5.2**](#52-prohibited-apps).

## Need Help?

If you have questions about any of our policies or need assistance:

* Message our

  live support
* Email us at [support@fanvue.com](mailto:support@fanvue.com)
* Visit our [Help Center](https://help.fanvue.com)

Fanvue reserves the right to amend this Policy from time to time. The latest version will always be accessible at [**legal.fanvue.com**](https://legal.fanvue.com), and significant changes will be communicated where required.