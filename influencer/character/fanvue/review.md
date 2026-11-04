# Fanvue policy review

* Reviewed: 2026-11-03T18:00:29-07:00 by the Nova fanvue policy agent (NIM)
* Documents re-fetched this run: acceptable-use-policy, api-policy, community-guidelines, content-moderation, creator-earnings-payouts, creator-terms, marketing-policy

## What changed in the machine rules

- updated rule minors_absolutely
- updated rule age_play_banned
- updated rule no_real_person_likeness
- updated rule no_ai_denial
- updated rule external_payments
- updated rule platform_migration
- updated rule in_person_solicitation
- updated rule extreme_themes
- agent dropped core rule explicit_text_house_rule; restored from previous policy
- updated rule weapons_drugs
- updated rule hate_violence_selfharm
- updated rule misleading_commerce
- updated rule pii_leak
- added rule ai_disclosure_required (source: CG D.1.1)
- added rule intimate_content_18_plus (source: CG C.6.1)
- added rule public_media_sfw (source: AUP 2.6.2.1, AUP 2.6.2.4)
- added rule no_impersonation (source: AUP 6.2, AUP 6.3)
- added rule no_illegal_violence (source: AUP 2.1.1)
- added rule no_unauthorized_commercial (source: AUP 2.4.5)
- added rule no_extortion (source: AUP 2.4.4)
- added rule no_self_referral (source: AUP 4.1)
- added rule under18_account (source: AUP 1.1)
- added rule fabricated_endorsements (source: MP 1.2.2, MP 1.2.4)
- added rule off_platform_transactions (source: MP 5.1.3)
- added rule illegal_exploitative_schemes (source: MP 5.1.2)
- added rule discriminatory_derogatory_language (source: MP 3.1, MP 3.2.1, MP 3.2.2)
- added rule unsafe_body_standards (source: MP 3.2.3)
- added rule ai_disclosure_media (source: API 4.3.1)
- added rule no_api_training (source: API 5.2.13)
- added rule no_moderation_evasion (source: API 5.1.5, API 5.2.11)
- added rule no_scraping_leaking (source: API 5.2.5)
- added rule no_gambling (source: API 5.2.1)
- added rule no_fake_engagement (source: API 5.2.8)
- added rule public_bio_sfw (source: CMP 1.6)
- added rule no_trust_or_client_money_claims (source: 2.8.6)
- added rule no_unpublished_promotional_rate_claims (source: 1.3)
- added rule standard_earning_rate_reference (source: 1.2)
- 8 required action(s) recorded

## Required actions for Eli

- [ ] Add clear and prominent AI disclosure to all AI-generated media on caption, message, and bio surfaces (CG D.1.1).
- [ ] Mark any intimate content as 18+ and ensure no CG C.6.3.3 prohibited acts appear (CG C.6.1, CG C.6.3.3).
- [ ] Retain age verification documentation for all depicted individuals and block minor resemblance in AI outputs (CG C.6.3.3.3, CG D.1.3).
- [ ] Deduplicate rules and re-source all citations to this Community Guidelines document only.
- [ ] Add clear AI-generated disclosure to all Nova content, public media, and prompts per AUP 2.3.1.4, AUP 2.5.2, AUP 2.6.2.6
- [ ] Ensure all content is authentically produced by the Creator or approved collaborators per AUP 2.3.1.1
- [ ] Keep public bio, banners, and profile media SFW with no nudity, implied nudity, or age-baiting per AUP 2.6.2
- [ ] Never arrange or promote in-person meetings; remove any such offers per AUP 2.4.6

## Agent notes

[community-guidelines] Dropped citations to identity.md, AUP, API, CM, and MP not present in this document; removed duplicate explicit_text_house_rule; preserved zero-tolerance minor and AI-disclosure rules without softening.

[acceptable-use-policy] Dropped explicit_text_house_rule: not grounded in this AUP (stricter house rule from identity.md). Retained minors, AI-disclosure, and in-person rules without softening. Added public-media SFW, impersonation, illegal/violence, commercial, extortion, referral, and under-18 account rules from this AUP.

[creator-terms] Dropped explicit_text_house_rule: not grounded in this Creator Terms document. This document does not define explicit content or include AI-disclosure text rules; only CT 5 off-platform transactions is directly text-enforceable here.

[marketing-policy] Existing explicit_text_house_rule excluded: not grounded in marketing-policy (sourced from identity.md/CG). Off-platform-payment zero-tolerance retained per MP 5.1.3 and not softened. AI-disclosure and minors rules originate outside this document and remain in force.

[api-policy] Dropped explicit_text_house_rule: cites identity.md/CG Definitions, not this api-policy document. All retained and added rules cite api-policy sections only.

[content-moderation] Dropped explicit_text_house_rule: not grounded in this document (CMP regulates explicit content via KYC declaration and AI scanning, not a total ban). Re-sourced minors and weapons/drugs rules to CMP sections. Added public_bio_sfw per CMP 1.6.

[creator-earnings-payouts] Existing explicit_text_house_rule is not grounded in creator-earnings-payouts and is dropped. This document covers earnings, payouts, taxes, refunds, chargebacks, and dormant balances only.
